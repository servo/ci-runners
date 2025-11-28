#[path = "impl_utm_backend.rs"]
mod backend;

use std::{
    collections::BTreeSet,
    net::Ipv4Addr,
    path::Path,
    sync::LazyLock,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};
use jane_eyre::eyre::{self, bail};
use settings::TOML;
use settings::profile::Profile;
use tracing::info;

pub(crate) struct Channel<T> {
    pub sender: Sender<T>,
    pub receiver: Receiver<T>,
}
pub(crate) static UTM_REQUEST: LazyLock<Channel<UtmRequest>> = LazyLock::new(|| {
    let (sender, receiver) = crossbeam_channel::bounded(0);
    Channel { sender, receiver }
});

#[derive(Debug)]
pub(crate) enum UtmRequest {
    ListGuests {
        result: Sender<eyre::Result<Vec<String>>>,
    },
    GuestStatus {
        result: Sender<eyre::Result<String>>,
        guest_name: String,
    },
    StartGuest {
        result: Sender<eyre::Result<()>>,
        guest_name: String,
    },
    DeleteGuest {
        result: Sender<eyre::Result<()>>,
        guest_name: String,
    },
    CloneGuest {
        result: Sender<eyre::Result<()>>,
        original_guest_name: String,
        new_guest_name: String,
    },
    RenameGuest {
        result: Sender<eyre::Result<()>>,
        old_guest_name: String,
        new_guest_name: String,
    },
}

pub fn initialise() -> eyre::Result<()> {
    self::backend::request_automation_permission()
}

pub fn handle_main_thread_request() -> eyre::Result<()> {
    if let Ok(request) = UTM_REQUEST.receiver.recv_timeout(Duration::from_secs(1)) {
        match request {
            UtmRequest::ListGuests { result } => result.send(self::backend::list_guests())?,
            UtmRequest::GuestStatus { result, guest_name } => {
                result.send(self::backend::guest_status(&guest_name))?
            }
            UtmRequest::StartGuest { result, guest_name } => {
                result.send(self::backend::start_guest(&guest_name))?
            }
            UtmRequest::DeleteGuest { result, guest_name } => {
                result.send(self::backend::delete_guest(&guest_name))?
            }
            UtmRequest::CloneGuest {
                result,
                original_guest_name,
                new_guest_name,
            } => result.send(self::backend::clone_guest(
                &original_guest_name,
                &new_guest_name,
            ))?,
            UtmRequest::RenameGuest {
                result,
                old_guest_name,
                new_guest_name,
            } => result.send(self::backend::rename_guest(
                &old_guest_name,
                &new_guest_name,
            ))?,
        }
    }
    Ok(())
}

pub fn list_template_guests() -> eyre::Result<Vec<String>> {
    // Output is not filtered by prefix, so we must filter it ourselves.
    let prefix = format!("{}-", TOML.libvirt_template_guest_prefix());
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST
        .sender
        .send(UtmRequest::ListGuests { result: tx })?;
    let result = rx
        .recv()??
        .into_iter()
        .filter(|name| name.starts_with(&prefix));

    Ok(result.collect())
}

pub fn list_rebuild_guests() -> eyre::Result<Vec<String>> {
    // Output is not filtered by prefix, so we must filter it ourselves.
    let prefix = format!("{}-", TOML.libvirt_rebuild_guest_prefix());
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST
        .sender
        .send(UtmRequest::ListGuests { result: tx })?;
    let result = rx
        .recv()??
        .into_iter()
        .filter(|name| name.starts_with(&prefix));

    Ok(result.collect())
}

pub fn list_runner_guests() -> eyre::Result<Vec<String>> {
    // Output is not filtered by prefix, so we must filter it ourselves.
    let prefix = format!("{}-", TOML.libvirt_runner_guest_prefix());
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST
        .sender
        .send(UtmRequest::ListGuests { result: tx })?;
    let result = rx
        .recv()??
        .into_iter()
        .filter(|name| name.starts_with(&prefix));

    Ok(result.collect())
}

#[expect(unused)]
pub fn update_screenshot(guest_name: &str, output_dir: &Path) -> eyre::Result<()> {
    bail!("TODO")
}

#[expect(unused)]
pub fn take_screenshot(guest_name: &str, output_path: &Path) -> eyre::Result<()> {
    bail!("TODO")
}

#[expect(unused)]
pub fn get_ipv4_address(guest_name: &str) -> Option<Ipv4Addr> {
    // TODO
    None
}

pub fn start_guest(guest_name: &str) -> eyre::Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST.sender.send(UtmRequest::StartGuest {
        result: tx,
        guest_name: guest_name.to_owned(),
    })?;
    Ok(rx.recv()??)
}

pub fn wait_for_guest(guest_name: &str, timeout: Duration) -> eyre::Result<()> {
    let timeout_for_log = timeout.as_secs();
    info!("Waiting for guest to shut down (max {timeout_for_log} seconds)");
    let start_time = Instant::now();
    while Instant::now()
        .checked_duration_since(start_time)
        .is_none_or(|d| d < timeout)
    {
        if guest_status(guest_name)? == "stopped" {
            return Ok(());
        }
    }

    bail!("Waiting for guest timed out!")
}

pub fn rename_guest(old_guest_name: &str, new_guest_name: &str) -> eyre::Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST.sender.send(UtmRequest::RenameGuest {
        result: tx,
        old_guest_name: old_guest_name.to_owned(),
        new_guest_name: new_guest_name.to_owned(),
    })?;
    Ok(rx.recv()??)
}

pub fn delete_guest(guest_name: &str) -> eyre::Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST.sender.send(UtmRequest::DeleteGuest {
        result: tx,
        guest_name: guest_name.to_owned(),
    })?;
    Ok(rx.recv()??)
}

#[expect(unused)]
pub fn prune_base_image_files(
    profile: &Profile,
    keep_snapshots: BTreeSet<String>,
) -> eyre::Result<()> {
    // Do nothing (not applicable to UTM)
    Ok(())
}

fn guest_status(guest_name: &str) -> eyre::Result<String> {
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST.sender.send(UtmRequest::GuestStatus {
        result: tx,
        guest_name: guest_name.to_owned(),
    })?;
    Ok(rx.recv()??)
}
