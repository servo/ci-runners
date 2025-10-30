#[cfg_attr(not(target_os = "macos"), path = "utm_dummy.rs")]
#[cfg_attr(target_os = "macos", path = "utm_macos.rs")]
mod platform;

use std::{sync::LazyLock, time::Duration};

use crossbeam_channel::Sender;
use jane_eyre::eyre;

use crate::Channel;

pub static UTM_REQUEST: LazyLock<Channel<UtmRequest>> = LazyLock::new(|| {
    let (sender, receiver) = crossbeam_channel::bounded(0);
    Channel { sender, receiver }
});

#[derive(Debug)]
pub enum UtmRequest {
    ListRunnerGuests {
        result: Sender<eyre::Result<Vec<String>>>,
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
}

pub fn handle_utm_request() -> eyre::Result<()> {
    if let Ok(request) = UTM_REQUEST.receiver.recv_timeout(Duration::from_secs(1)) {
        match request {
            UtmRequest::ListRunnerGuests { result } => {
                result.send(self::platform::list_runner_guests())?
            }
            UtmRequest::DeleteGuest { result, guest_name } => {
                result.send(self::platform::delete_guest(&guest_name))?
            }
            UtmRequest::CloneGuest {
                result,
                original_guest_name,
                new_guest_name,
            } => result.send(self::platform::clone_guest(
                &original_guest_name,
                &new_guest_name,
            ))?,
        }
    }
    Ok(())
}

#[cfg_attr(not(target_os = "macos"), expect(unused_imports))]
pub use self::platform::request_automation_permission;

#[cfg_attr(not(target_os = "macos"), expect(dead_code))]
pub fn list_runner_guests() -> eyre::Result<Vec<String>> {
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST
        .sender
        .send(UtmRequest::ListRunnerGuests { result: tx })?;
    Ok(rx.recv()??)
}

#[expect(dead_code)]
pub fn delete_guest(guest_name: &str) -> eyre::Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST.sender.send(UtmRequest::DeleteGuest {
        result: tx,
        guest_name: guest_name.to_owned(),
    })?;
    Ok(rx.recv()??)
}

#[expect(dead_code)]
pub fn clone_guest(original_guest_name: &str, new_guest_name: &str) -> eyre::Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST.sender.send(UtmRequest::CloneGuest {
        result: tx,
        original_guest_name: original_guest_name.to_owned(),
        new_guest_name: new_guest_name.to_owned(),
    })?;
    Ok(rx.recv()??)
}
