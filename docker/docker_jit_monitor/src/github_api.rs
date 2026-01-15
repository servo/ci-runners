use std::process::{self, Command, Output};

use log::{debug, warn};
use serde_json::Value;

use crate::{RunnerConfig, SpawnRunnerError};

/// Function to call the api. Raw just is used spawnrunner.
/// This gives you the _executed_ cmd.
/// Notice that the api_endpoint needs a slash before it. The api is very peculious
/// with slashes and this is the easiest
/// Note: octocrab apparently requires more coarse grained tokens compared
/// to `gh`, so we use `gh`.
fn call_github_runner_api(
    ci_scope: &str,
    method: &str,
    api_endpoint: &str,
    raw_fields: &[String],
    fields: &[String],
) -> Result<Output, SpawnRunnerError> {
    // Note: octocrab apparently requires more coarse grained tokens compared to `gh`, so we use `gh`.
    let mut cmd = Command::new("gh");
    let final_api_endpoint = format!("{}/actions/runners/{}", ci_scope, api_endpoint);
    cmd.args([
        "api",
        "--method",
        method,
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "X-GitHub-Api-Version: 2022-11-28",
        &final_api_endpoint,
    ]);
    for value in raw_fields {
        cmd.arg("--raw-field").arg(value);
    }

    for value in fields {
        cmd.arg("--field").arg(value);
    }

    let output = cmd.output().map_err(SpawnRunnerError::SpawnGhError)?;
    Ok(output)
}

// todo: add arg for optional device to pass into the runner
pub(crate) fn spawn_runner(config: &RunnerConfig) -> Result<process::Child, SpawnRunnerError> {
    let mut raw_fields = config
        .labels
        .iter()
        .map(|label| format!("labels[]={label}"))
        .collect::<Vec<String>>();
    raw_fields.push(format!("name={}", config.name));
    raw_fields.push(format!("work_folder={}", config.work_folder));
    let fields = [format!("runner_group_id={}", config.runner_group_id)];

    let output = call_github_runner_api(
        &config.servo_ci_scope,
        "POST",
        "generate-jitconfig",
        &raw_fields,
        &fields,
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(SpawnRunnerError::GhApiError(
            output.status.code().unwrap_or(-1),
            stderr,
        ));
    }

    let registration_info = String::from_utf8(output.stdout)?;
    let registration_info: Value = serde_json::from_str(&registration_info)?;
    let Some(encoded_jit_config) = &registration_info
        .get("encoded_jit_config")
        .and_then(|v| v.as_str())
    else {
        return Err(SpawnRunnerError::EncodedJitConfigNotFound);
    };
    if let Some(id) = registration_info.get("runner").and_then(|v| v.get("id")) {
        debug!("The GitHub runner id: is {id} ");
    } else {
        warn!("Couldn't find runner.id in the GitHub API answer");
    }

    let mut cmd = std::process::Command::new("docker");
    cmd.arg("run").arg("--rm");

    if let Some(device) = &config.map_device {
        cmd.arg("--device").arg(device);
    }

    // Start the gh runner inside the container
    cmd.arg(&config.docker_image_and_tag)
        .arg("/home/servo_ci/runner/run.sh")
        .arg(" --jitconfig")
        .arg(encoded_jit_config);

    let runner = cmd.spawn().map_err(SpawnRunnerError::SpawnDockerError)?;
    Ok(runner)
}
