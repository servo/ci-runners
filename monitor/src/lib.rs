pub mod github;

use std::collections::BTreeMap;

use chrono::Utc;
use jane_eyre::eyre::eyre;
use settings::TOML;
use web::rocket_eyre::{self, EyreReport};

use crate::github::{download_artifact_string, list_workflow_run_artifacts};

pub fn validate_tokenless_select(
    unique_id: &str,
    qualified_repo: &str,
    run_id: &str,
) -> rocket_eyre::Result<String> {
    if !qualified_repo.starts_with(&TOML.allowed_qualified_repo_prefix) {
        Err(EyreReport::InternalServerError(eyre!(
            "Not allowed on this `qualified_repo`"
        )))?;
    }
    let artifacts = list_workflow_run_artifacts(&qualified_repo, &run_id)?;
    let args_artifact = format!("servo-ci-runners_{unique_id}");
    let Some(args_artifact) = artifacts
        .into_iter()
        .find(|artifact| artifact.name == args_artifact)
    else {
        Err(EyreReport::InternalServerError(eyre!(
            "No args artifact found: {args_artifact}"
        )))?
    };
    let artifact_age = Utc::now().signed_duration_since(args_artifact.created_at);
    if artifact_age > TOML.tokenless_select_artifact_max_age() {
        Err(EyreReport::InternalServerError(eyre!(
            "Args artifact is too old ({}): {}",
            artifact_age,
            args_artifact.name,
        )))?
    }
    let args_artifact = download_artifact_string(&args_artifact.archive_download_url)?;
    let mut args = args_artifact
        .lines()
        .flat_map(|line| line.split_once("="))
        .collect::<BTreeMap<&str, &str>>();
    if args.remove("unique_id") != Some(&*unique_id) {
        Err(EyreReport::InternalServerError(eyre!(
            "Wrong unique_id in artifact"
        )))?;
    }
    if args.remove("qualified_repo") != Some(&*qualified_repo) {
        Err(EyreReport::InternalServerError(eyre!(
            "Wrong qualified_repo in artifact"
        )))?;
    }
    if args.remove("run_id") != Some(&*run_id) {
        Err(EyreReport::InternalServerError(eyre!(
            "Wrong run_id in artifact"
        )))?;
    }
    let Some(profile_key) = args.remove("self_hosted_profile") else {
        Err(EyreReport::InternalServerError(eyre!(
            "Wrong run_id in artifact"
        )))?
    };
    Ok(profile_key.to_owned())
}
