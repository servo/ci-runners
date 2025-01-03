use core::str;
use std::{
    collections::BTreeMap,
    io::ErrorKind,
    mem::take,
    path::Path,
    thread::{self, JoinHandle},
    time::Duration,
};

use chrono::{SecondsFormat, Utc};
use jane_eyre::eyre::{self, bail, OptionExt};
use subprocess::{CommunicateError, Exec, Redirection};
use tracing::{error, info, warn};

use crate::{profile::Profile, runner::Runners, DOTENV};

#[derive(Debug, Default)]
pub struct Rebuilds {
    cached_servo_repo_update: Option<JoinHandle<eyre::Result<()>>>,
    rebuilds: BTreeMap<String, Rebuild>,
}

#[derive(Debug)]
struct Rebuild {
    thread: JoinHandle<eyre::Result<()>>,
    snapshot_name: String,
}

impl Rebuilds {
    pub fn run(
        &mut self,
        profiles: &mut BTreeMap<String, Profile>,
        runners: &Runners,
    ) -> eyre::Result<()> {
        let mut profiles_needing_rebuild = BTreeMap::default();
        let mut cached_servo_repo_was_just_updated = false;

        // Reap the Servo update thread, if needed.
        if let Some(thread) = self.cached_servo_repo_update.take() {
            if thread.is_finished() {
                match thread.join() {
                    Ok(Ok(())) => {
                        info!("Servo update thread exited");
                        cached_servo_repo_was_just_updated = true;
                    }
                    Ok(Err(report)) => error!(%report, "Servo update thread error"),
                    Err(panic) => error!(?panic, "Servo update thread panic"),
                };
            } else {
                self.cached_servo_repo_update = Some(thread);
            }
        }

        // Determine which profiles need their images rebuilt.
        for (key, profile) in profiles.iter() {
            let needs_rebuild = profile.image_needs_rebuild();
            if needs_rebuild.unwrap_or(true) {
                let runner_count = profile.runners(&runners).count();
                if needs_rebuild.is_none() {
                    info!("profile {key}: image may or may not need rebuild");
                } else if self.cached_servo_repo_update.is_some() {
                    info!( "profile {key}: image needs rebuild; cached Servo repo update still running" );
                } else if self.rebuilds.contains_key(key) {
                    info!("profile {key}: image needs rebuild; image rebuild still running");
                } else if runner_count > 0 {
                    info!(
                        runner_count,
                        "profile {key}: image needs rebuild; waiting for runners"
                    );
                } else {
                    info!("profile {key}: image needs rebuild");
                    profiles_needing_rebuild.insert(key, profile);
                }
            }
        }

        // If we’re kicking off image rebuilds, update our cached Servo repo if there are no
        // rebuilds already running that might read from it.
        if self.rebuilds.is_empty()
            && !profiles_needing_rebuild.is_empty()
            && !cached_servo_repo_was_just_updated
        {
            assert!(self.cached_servo_repo_update.is_none());

            // Kick off a Servo update thread. Don’t start any image rebuild threads.
            info!("Updating our cached Servo repo, before we start image rebuilds");
            self.cached_servo_repo_update = Some(thread::spawn(servo_update_thread));
            return Ok(());
        }

        // Kick off image rebuild threads for profiles needing it.
        for (key, profile) in profiles_needing_rebuild {
            let build_script_path = Path::new(&profile.configuration_name).join("build-image.sh");
            let snapshot_name = Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true);

            let key_for_thread = key.clone();
            let snapshot_name_for_thread = snapshot_name.clone();
            let thread = thread::spawn(move || {
                rebuild_thread(
                    &key_for_thread,
                    build_script_path,
                    &snapshot_name_for_thread,
                )
            });

            self.rebuilds.insert(
                key.to_owned(),
                Rebuild {
                    thread,
                    snapshot_name: snapshot_name.clone(),
                },
            );
        }

        // Reap image rebuild threads, updating the profile on success.
        let mut remaining_rebuilds = BTreeMap::default();
        for (profile_key, rebuild) in take(&mut self.rebuilds) {
            if rebuild.thread.is_finished() {
                match rebuild.thread.join() {
                    Ok(Ok(())) => {
                        info!(profile_key, "Image rebuild thread exited");
                        let profile = profiles
                            .get_mut(&profile_key)
                            .ok_or_eyre("Failed to get profile")?;
                        profile.base_image_snapshot = rebuild.snapshot_name;
                    }
                    Ok(Err(report)) => error!(profile_key, %report, "Image rebuild thread error"),
                    Err(panic) => error!(profile_key, ?panic, "Image rebuild thread panic"),
                };
            } else {
                remaining_rebuilds.insert(profile_key, rebuild);
            }
        }
        self.rebuilds.extend(remaining_rebuilds);

        Ok(())
    }
}

#[tracing::instrument]
fn servo_update_thread() -> eyre::Result<()> {
    info!("Starting repo update");
    fn git() -> Exec {
        Exec::cmd("git").cwd(&DOTENV.main_repo_path)
    }

    run_and_log_output_as_info(git().args(&["reset", "--hard"]))?;
    run_and_log_output_as_info(git().args(&["fetch", "origin", "main"]))?;
    run_and_log_output_as_info(git().args(&["switch", "--detach", "FETCH_HEAD"]))?;

    Ok(())
}

#[tracing::instrument(skip(build_script_path, snapshot_name))]
fn rebuild_thread(
    profile_key: &str,
    build_script_path: impl AsRef<Path>,
    snapshot_name: &str,
) -> eyre::Result<()> {
    info!(build_script_path = ?build_script_path.as_ref(), ?snapshot_name, "Starting image rebuild");
    let exec = Exec::cmd(build_script_path.as_ref())
        .cwd("..")
        .arg(snapshot_name);

    run_and_log_output_as_info(exec)
}

fn run_and_log_output_as_info(exec: Exec) -> eyre::Result<()> {
    let mut child = exec
        .stdout(Redirection::Pipe)
        .stderr(Redirection::Merge)
        .popen()?;
    let mut communicator = child
        .communicate_start(None)
        .limit_time(Duration::from_secs(1));
    let exit_status = loop {
        match communicator.read() {
            Err(error) if error.kind() != ErrorKind::TimedOut => {
                warn!(?error, "Error reading from child process");
                break child.wait()?;
            }
            // Err(empty) or Err(non-empty) means we timed out, and there may be more output in future.
            // Ok(non-empty) means we got some output. Hopefully this avoids giving us partial lines?
            // Ok(empty) means there is definitely no more output.
            ref result @ (Ok(ref capture) | Err(CommunicateError { ref capture, .. })) => {
                let (Some(stdout), None) = capture else {
                    unreachable!("Guaranteed by child definition")
                };
                if result.is_ok() && stdout.is_empty() {
                    // There is definitely no more output
                    break child.wait()?;
                } else if !stdout.is_empty() {
                    for line in stdout.split(|&b| b == b'\n') {
                        let line = str::from_utf8(line).map_err(|_| line);
                        match line {
                            Ok(string) => info!(line = %string),
                            Err(bytes) => info!(?bytes),
                        }
                    }
                }
            }
        }
    };
    if !exit_status.success() {
        bail!("Command exited with status {:?}", exit_status);
    }

    Ok(())
}
