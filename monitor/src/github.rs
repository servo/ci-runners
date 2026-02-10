use std::{
    fmt::Debug,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, FixedOffset};
use cmd_lib::{run_cmd, run_fun};
use jane_eyre::eyre::{self, Context};
use serde::{Deserialize, Serialize};
use serde_json::json;
use settings::{DOTENV, TOML};
use tracing::trace;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiRunner {
    pub id: usize,
    pub busy: bool,
    pub name: String,
    pub status: String,
    pub labels: Vec<ApiRunnerLabel>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiGenerateJitconfigResponse {
    pub runner: ApiRunner,
    pub encoded_jit_config: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiRunnerLabel {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiWorkflowRunArtifactsResponse {
    pub artifacts: Vec<ApiArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiArtifact {
    pub name: String,
    pub created_at: DateTime<FixedOffset>,
    pub archive_download_url: String,
}

impl ApiRunner {
    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.labels.iter().map(|label| label.name.as_str())
    }

    pub fn label_with_key(&self, key: &str) -> Option<&str> {
        self.labels()
            .find_map(|label| label.strip_prefix(&format!("{key}:")))
    }
}

/// Caches responses for a while, to avoid hitting REST API rate limits.
#[derive(Debug, Default)]
pub struct Cache<Response> {
    inner: Option<CacheData<Response>>,
    forced_expiry: Option<Instant>,
}

#[derive(Debug)]
struct CacheData<Response> {
    response: Response,
    cached_at: Instant,
}

impl<Response: Clone + Debug> Cache<Response> {
    pub fn get(&mut self, miss: impl FnOnce() -> eyre::Result<Response>) -> eyre::Result<Response> {
        if let Some(cached) = &mut self.inner {
            let now = Instant::now();
            let age = now.duration_since(cached.cached_at);
            if age >= TOML.api_cache_timeout() {
                trace!(?age, "Cache expired");
                self.invalidate();
            } else if self.forced_expiry.is_some_and(|e| now >= e) {
                trace!(?self.forced_expiry, ?now, "Cache reached forced expiry");
                self.invalidate();
            } else {
                trace!(?age, ?cached.response, "Cache hit");
                return Ok(cached.response.clone());
            }
        }

        trace!("Cache miss");
        let response = miss()?;
        self.inner = Some(CacheData {
            response: response.clone(),
            cached_at: Instant::now(),
        });

        Ok(response)
    }

    pub fn invalidate(&mut self) {
        self.inner.take();
        self.forced_expiry.take();
    }

    pub fn invalidate_in(&mut self, duration: Duration) {
        let forced_expiry = Instant::now() + duration;
        if self.forced_expiry.is_none_or(|e| forced_expiry < e) {
            self.forced_expiry = Some(forced_expiry);
        }
    }
}

fn list_registered_runners() -> eyre::Result<Vec<ApiRunner>> {
    let github_or_forgejo_token = &DOTENV.github_or_forgejo_token;
    let github_api_scope_url = &TOML.github_api_scope_url;
    let result = if TOML.github_api_is_forgejo {
        // FIXME: this leaks the token in logs when the command fails
        run_fun!(curl -fsSH "Authorization: token $github_or_forgejo_token"
            "$github_api_scope_url/actions/runners" // TODO: pagination?
            | jq -er ".runners")?
    } else {
        run_fun!(GITHUB_TOKEN=$github_or_forgejo_token gh api
            -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28"
            "$github_api_scope_url/actions/runners" --paginate -q ".runners[]"
            | jq -s .)?
    };

    Ok(serde_json::from_str(&result).wrap_err("Failed to parse JSON")?)
}

pub fn list_registered_runners_for_host() -> eyre::Result<Vec<ApiRunner>> {
    let prefix = format!("{}-", TOML.libvirt_runner_guest_prefix());
    let suffix = format!("@{}", TOML.github_api_suffix);
    let result = list_registered_runners()?
        .into_iter()
        .filter(|runner| runner.name.starts_with(&prefix))
        .filter(|runner| runner.name.ends_with(&suffix));

    Ok(result.collect())
}

pub fn register_runner(runner_name: &str, label: &str, work_folder: &str) -> eyre::Result<String> {
    let github_or_forgejo_token = &DOTENV.github_or_forgejo_token;
    let github_api_scope_url = &TOML.github_api_scope_url;
    let github_api_suffix = &TOML.github_api_suffix;
    let result = if TOML.github_api_is_forgejo {
        // FIXME: this leaks the token in logs when the command fails
        let registration_token = run_fun!(curl -fsSH "Authorization: token $github_or_forgejo_token"
            -X POST "$github_api_scope_url/actions/runners/registration-token"
            | jq -er .token)?;

        // Hit the internal(?) registration API using JSON instead of protobuf (<https://connectrpc.com>):
        // <https://code.forgejo.org/forgejo/actions-proto/src/commit/1b2c1084d34619fbb6cb768741a9321c49956032/proto/runner/v1/messages.proto>
        // <https://codeberg.org/forgejo/forgejo/src/commit/ffbd500600d45fd86805004694086faaf68ecbdb/routers/api/actions/runner/runner.go>
        #[derive(Clone, Debug, Deserialize, Serialize)]
        struct RegisterRequest {
            name: String,
            token: String,
            version: String,
            labels: Vec<String>,
            ephemeral: bool,
        }
        #[derive(Clone, Debug, Deserialize, Serialize)]
        struct ForgejoApiRegisterResponse {
            runner: ForgejoApiRegisterResponseRunner,
        }
        #[derive(Clone, Debug, Deserialize, Serialize)]
        struct ForgejoApiRegisterResponseRunner {
            id: String,
            uuid: String,
            token: String,
            name: String,
            version: String,
            labels: Vec<String>,
        }
        impl ForgejoApiRegisterResponseRunner {
            /// Convert the runner to the `.runner` format expected by forgejo-runner.
            fn to_forgejo_dot_runner(&self) -> eyre::Result<String> {
                let id = usize::from_str_radix(&self.id, 10)?;
                let address = TOML.github_api_scope_url.join("/")?.to_string();
                let address = address.strip_suffix("/").expect("Guaranteed by argument");
                Ok(serde_json::to_string(&json!({
                    "id": id,
                    "uuid": self.uuid,
                    "name": self.name,
                    "token": self.token,
                    "address": address,
                    "labels": self.labels,
                }))?)
            }
        }
        let request = RegisterRequest {
            name: format!("{runner_name}@{github_api_suffix}"),
            token: registration_token,
            version: "ServoCI".to_owned(),
            labels: vec!["self-hosted".to_owned(), label.to_owned()],
            ephemeral: true,
        };
        let request = serde_json::to_string(&request)?;
        let register_url =
            github_api_scope_url.join("/api/actions/runner.v1.RunnerService/Register")?;
        let response = run_fun!(curl -fsSH "Content-Type: application/json"
            --data-raw $request "$register_url")?;
        let response: ForgejoApiRegisterResponse = serde_json::from_str(&response)?;
        response.runner.to_forgejo_dot_runner()?
    } else {
        let response = run_fun!(GITHUB_TOKEN=$github_or_forgejo_token gh api
            -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28"
            --method POST "$github_api_scope_url/actions/runners/generate-jitconfig"
            -f "name=$runner_name@$github_api_suffix" -F "runner_group_id=1" -f "work_folder=$work_folder"
            -f "labels[]=self-hosted" -f "labels[]=X64" -f "labels[]=$label")?;
        let response: ApiGenerateJitconfigResponse = serde_json::from_str(&response)?;
        response.encoded_jit_config
    };

    Ok(result)
}

pub fn unregister_runner(id: usize) -> eyre::Result<()> {
    let github_or_forgejo_token = &DOTENV.github_or_forgejo_token;
    let github_api_scope_url = &TOML.github_api_scope_url;
    if TOML.github_api_is_forgejo {
        // FIXME: this leaks the token in logs when the command fails
        run_cmd!(curl -fsSH "Authorization: token $github_or_forgejo_token"
            -X DELETE "$github_api_scope_url/actions/runners/$id")?;
    } else {
        run_cmd!(GITHUB_TOKEN=$github_or_forgejo_token gh api
            -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28"
            --method DELETE "$github_api_scope_url/actions/runners/$id")?;
    }

    Ok(())
}

pub fn reserve_runner(
    id: usize,
    unique_id: &str,
    reserved_since: SystemTime,
    reserved_by: &str,
) -> eyre::Result<()> {
    let github_or_forgejo_token = &DOTENV.github_or_forgejo_token;
    let github_api_scope_url = &TOML.github_api_scope_url;
    let reserved_since = reserved_since.duration_since(UNIX_EPOCH)?.as_secs();
    if TOML.github_api_is_forgejo {
        todo!()
    } else {
        run_cmd!(GITHUB_TOKEN=$github_or_forgejo_token gh api
            -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28"
            --method POST "$github_api_scope_url/actions/runners/$id/labels"
            -f "labels[]=reserved-for:$unique_id"
            -f "labels[]=reserved-since:$reserved_since"
            -f "labels[]=reserved-by:$reserved_by")?;
    }

    Ok(())
}

pub fn list_workflow_run_artifacts(
    qualified_repo: &str,
    run_id: &str,
) -> eyre::Result<Vec<ApiArtifact>> {
    // FIXME: breaks if we have more than 100 artifacts
    let result = run_fun!(gh api -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28"
        "/repos/$qualified_repo/actions/runs/$run_id/artifacts?per_page=100")?;
    let result: ApiWorkflowRunArtifactsResponse =
        serde_json::from_str(&result).wrap_err("Failed to parse JSON")?;
    Ok(result.artifacts)
}

pub fn download_artifact_string(url: &str) -> eyre::Result<String> {
    Ok(run_fun!(gh api -- $url | funzip)?)
}
