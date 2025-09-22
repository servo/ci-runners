use std::{
    fmt::Debug,
    time::{Duration, Instant},
};

use cmd_lib::{run_cmd, run_fun};
use jane_eyre::eyre::{self, Context};
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::DOTENV;

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
            if age >= DOTENV.api_cache_timeout {
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
    let github_api_scope = &DOTENV.github_api_scope;
    let result = run_fun!(gh api -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28"
    "$github_api_scope/actions/runners" --paginate -q ".runners[]"
    | jq -s .)?;

    Ok(serde_json::from_str(&result).wrap_err("Failed to parse JSON")?)
}

pub fn list_registered_runners_for_host() -> eyre::Result<Vec<ApiRunner>> {
    let suffix = format!("@{}", DOTENV.github_api_suffix);
    let result = list_registered_runners()?
        .into_iter()
        .filter(|runner| runner.name.ends_with(&suffix));

    Ok(result.collect())
}

pub fn register_runner(runner_name: &str, label: &str, work_folder: &str) -> eyre::Result<String> {
    let github_api_suffix = &DOTENV.github_api_suffix;
    let github_api_scope = &DOTENV.github_api_scope;
    let result = run_fun!(gh api --method POST -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28"
    "$github_api_scope/actions/runners/generate-jitconfig"
    -f "name=$runner_name@$github_api_suffix" -F "runner_group_id=1" -f "work_folder=$work_folder"
    -f "labels[]=self-hosted" -f "labels[]=X64" -f "labels[]=$label")?;

    Ok(result)
}

pub fn unregister_runner(id: usize) -> eyre::Result<()> {
    let github_api_scope = &DOTENV.github_api_scope;
    run_cmd!(gh api --method DELETE -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28"
        "$github_api_scope/actions/runners/$id")?;

    Ok(())
}
