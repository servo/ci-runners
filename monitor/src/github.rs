use std::{
    fmt::Debug,
    process::{Command, Stdio},
    time::Instant,
};

use jane_eyre::eyre::{self, Context};
use log::trace;
use serde::Deserialize;

use crate::SETTINGS;

#[derive(Clone, Debug, Deserialize)]
pub struct ApiRunner {
    pub id: usize,
    pub busy: bool,
    pub name: String,
    pub status: String,
    pub labels: Vec<ApiRunnerLabel>,
}

#[derive(Clone, Debug, Deserialize)]
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
}

#[derive(Debug)]
struct CacheData<Response> {
    response: Response,
    cached_at: Instant,
}

impl<Response: Clone + Debug> Cache<Response> {
    pub fn get(&mut self, miss: impl FnOnce() -> eyre::Result<Response>) -> eyre::Result<Response> {
        if let Some(cached) = &mut self.inner {
            let age = Instant::now().duration_since(cached.cached_at);
            if age < SETTINGS.api_cache_timeout {
                trace!("Cache hit ({age:?} seconds old): {:?}", cached.response);
                return Ok(cached.response.clone());
            } else {
                trace!("Cache expired ({age:?} seconds old)");
                self.invalidate();
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
    }
}

fn list_registered_runners() -> eyre::Result<Vec<ApiRunner>> {
    let output = Command::new("../list-registered-runners.sh")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
    if output.status.success() {
        return serde_json::from_slice(&output.stdout).wrap_err("Failed to parse JSON");
    } else {
        eyre::bail!("Command exited with status {}", output.status);
    }
}

pub fn list_registered_runners_for_host() -> eyre::Result<Vec<ApiRunner>> {
    let suffix = format!("@{}", SETTINGS.github_api_suffix);
    let result = list_registered_runners()?
        .into_iter()
        .filter(|runner| runner.name.ends_with(&suffix));

    Ok(result.collect())
}
