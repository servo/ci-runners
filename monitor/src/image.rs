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

use crate::{profile::Profile, runner::Runners};

#[derive(Debug, Default)]
pub struct Rebuilds {
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
        // Kick off rebuilds for any profiles whose images are too old.
        for (key, profile) in profiles.iter() {
            let needs_rebuild = profile.image_needs_rebuild();
            if needs_rebuild.unwrap_or(true) {
                let runner_count = profile.runners(&runners).count();
                if needs_rebuild.is_none() {
                    info!(
                        key,
                        runner_count, "profile image may or may not need rebuild"
                    );
                } else if runner_count > 0 {
                    info!(
                        key,
                        runner_count, "profile image needs rebuild; waiting for runners"
                    );
                } else if self.rebuilds.contains_key(key) {
                    info!(
                        key,
                        runner_count, "profile image needs rebuild; image rebuild still running"
                    );
                } else {
                    info!(
                        key,
                        runner_count, "profile image needs rebuild; starting image rebuild now"
                    );
                    let build_script_path =
                        Path::new(&profile.configuration_name).join("build-image.sh");
                    let snapshot_name = Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true);
                    let cloned_snapshot_name = snapshot_name.clone();
                    let rebuild = Rebuild {
                        thread: thread::spawn(move || {
                            rebuild_thread(build_script_path, &cloned_snapshot_name)
                        }),
                        snapshot_name: snapshot_name.clone(),
                    };
                    self.rebuilds.insert(key.to_owned(), rebuild);
                }
            }
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

#[tracing::instrument(skip(build_script_path), fields(build_script_path = ?build_script_path.as_ref()))]
fn rebuild_thread(build_script_path: impl AsRef<Path>, snapshot_name: &str) -> eyre::Result<()> {
    let mut child = Exec::cmd(build_script_path.as_ref())
        .cwd("..")
        .arg(snapshot_name)
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
