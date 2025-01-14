use std::collections::BTreeMap;

use askama::Template;
use jane_eyre::eyre;
use serde_json::json;

use crate::{
    profile::{Profile, Profiles, RunnerCounts},
    runner::{Runner, Runners},
    TOML,
};

#[derive(Clone, Debug)]
pub struct Dashboard {
    pub json: String,
    pub html: String,
}

#[derive(Clone, Debug, Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate<'monitor> {
    profiles: &'monitor Profiles,
    profile_runner_counts: &'monitor BTreeMap<String, RunnerCounts>,
    runners: &'monitor Runners,
}

impl Dashboard {
    pub fn render(
        profiles: &Profiles,
        profile_runner_counts: &BTreeMap<String, RunnerCounts>,
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
        let html = DashboardTemplate {
            profiles,
            profile_runner_counts,
            runners,
        }
        .render()?;

        Ok(Self { json, html })
    }
}

impl DashboardTemplate<'_> {
    fn profile(&self, key: impl AsRef<str>) -> Option<&Profile> {
        self.profiles.get(key.as_ref())
    }

    fn status(&self, runner: &Runner) -> String {
        format!("{:?}", runner.status())
    }

    fn age(&self, runner: &Runner) -> eyre::Result<String> {
        runner.age().map(|age| format!("{:?}", age))
    }

    fn reserved_since(&self, runner: &Runner) -> eyre::Result<String> {
        Ok(format!("{:?}", runner.reserved_since()?))
    }

    fn labels(&self, runner: &Runner) -> Vec<String> {
        runner
            .registration()
            .iter()
            .flat_map(|r| r.labels())
            .map(|l| l.to_owned())
            .collect()
    }
}
