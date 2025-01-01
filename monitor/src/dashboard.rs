use std::collections::BTreeMap;

use jane_eyre::eyre;
use serde_json::json;

use crate::{profile::RunnerCounts, runner::Runners, TOML};

#[derive(Clone, Debug)]
pub struct Dashboard {
    pub json: String,
}

impl Dashboard {
    pub fn render(
        profile_runner_counts: &BTreeMap<&str, RunnerCounts>,
        runners: &Runners,
    ) -> eyre::Result<Self> {
        let json = serde_json::to_string(&json!({
            "profile_runner_counts": &profile_runner_counts,
            "runners": &runners
                .iter()
                .map(|(id, runner)| {
                    json!({
                        "id": id,
                        "screenshot_url": format!("{}runner/{id}/screenshot", TOML.external_base_url),
                        "runner": runner,
                    })
                })
                .collect::<Vec<_>>(),
        }))?;

        Ok(Self { json })
    }
}
