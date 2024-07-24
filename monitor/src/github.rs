use std::{
    env,
    process::{Command, Stdio},
};

use jane_eyre::eyre::{self, Context};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ApiRunner {
    pub id: usize,
    pub busy: bool,
    pub name: String,
    pub os: String,
    pub status: String,
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
    let suffix = format!("@{}", env::var("SERVO_CI_GITHUB_API_SUFFIX")?);
    let result = list_registered_runners()?
        .into_iter()
        .filter(|runner| runner.name.ends_with(&suffix));

    Ok(result.collect())
}
