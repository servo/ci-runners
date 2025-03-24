use core::str;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read},
    mem::take,
    path::Path,
    thread::{self, JoinHandle},
};

use chrono::{SecondsFormat, Utc};
use cmd_lib::spawn_with_output;
use jane_eyre::eyre;
use tracing::{error, info, warn};

use crate::{profile::Profiles, runner::Runners, DOTENV, LIB_MONITOR_DIR};

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
    pub fn run(&mut self, profiles: &mut Profiles, runners: &Runners) -> eyre::Result<()> {
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
            let needs_rebuild = profiles.image_needs_rebuild(profile);
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
            let build_script_path = Path::new(&*LIB_MONITOR_DIR)
                .join(&profile.configuration_name)
                .join("build-image.sh");
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
                        profiles.set_base_image_snapshot(&profile_key, &rebuild.snapshot_name)?;
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

    let main_repo_path = &DOTENV.main_repo_path;
    let pipe = || |reader| log_output_as_info(reader);
    spawn_with_output!(git -C $main_repo_path reset --hard 2>&1)?.wait_with_pipe(&mut pipe())?;
    spawn_with_output!(git -C $main_repo_path fetch origin main 2>&1)?
        .wait_with_pipe(&mut pipe())?;
    spawn_with_output!(git -C $main_repo_path switch --detach FETCH_HEAD 2>&1)?
        .wait_with_pipe(&mut pipe())?;

    Ok(())
}

#[tracing::instrument(skip(build_script_path, snapshot_name))]
fn rebuild_thread(
    profile_key: &str,
    build_script_path: impl AsRef<Path>,
    snapshot_name: &str,
) -> eyre::Result<()> {
    let build_script_path = build_script_path.as_ref();
    info!(build_script_path = ?build_script_path, ?snapshot_name, "Starting image rebuild");
    let pipe = || |reader| log_output_as_info(reader);
    spawn_with_output!($build_script_path $snapshot_name 2>&1)?.wait_with_pipe(&mut pipe())?;

    Ok(())
}

/// Log the given output to tracing.
///
/// Unlike cmd_lib’s built-in logging:
/// - it handles CR-based progress output correctly, such as in `curl`, `dd`, and `rsync`
/// - it uses `tracing` instead of `log`, so the logs show the correct target and any span context
///   given via `#[tracing::instrument]`
///
/// This only works with `spawn_with_output!()`, and `wait_with_pipe()` only works with stdout, so
/// if you want to log both stdout and stderr, use `spawn_with_output!(... 2>&1)`.
fn log_output_as_info(reader: Box<dyn Read>) {
    let mut reader = BufReader::new(reader);
    let mut buffer = vec![];
    loop {
        // Unconditionally try to read more data, since the BufReader buffer is empty
        let result = match reader.fill_buf() {
            Ok(buffer) => buffer,
            Err(error) => {
                warn!(?error, "Error reading from child process");
                break;
            }
        };
        // Add the result onto our own buffer
        buffer.extend(result);
        // Empty the BufReader
        let read_len = result.len();
        reader.consume(read_len);

        // Log output to tracing. Take whole “lines” at every LF or CR (for progress bars etc),
        // but leave any incomplete lines in our buffer so we can try to complete them.
        while let Some(offset) = buffer.iter().position(|&b| b == b'\n' || b == b'\r') {
            let line = &buffer[..offset];
            let line = str::from_utf8(line).map_err(|_| line);
            match line {
                Ok(string) => info!(line = %string),
                Err(bytes) => info!(?bytes),
            }
            buffer = buffer.split_off(offset + 1);
        }

        if read_len == 0 {
            break;
        }
    }

    // Log any remaining incomplete line to tracing.
    if !buffer.is_empty() {
        let line = &buffer;
        let line = str::from_utf8(line).map_err(|_| line);
        match line {
            Ok(string) => info!(line = %string),
            Err(bytes) => info!(?bytes),
        }
    }
}
