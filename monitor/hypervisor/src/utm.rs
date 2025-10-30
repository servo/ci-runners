use jane_eyre::eyre;

#[cfg(target_os = "macos")]
use crate::platform::{UTM_REQUEST, UtmRequest};

#[cfg(not(target_os = "macos"))]
#[expect(unused)]
pub fn clone_guest(original_guest_name: &str, new_guest_name: &str) -> eyre::Result<()> {
    unimplemented!()
}

#[cfg(target_os = "macos")]
pub fn clone_guest(original_guest_name: &str, new_guest_name: &str) -> eyre::Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(0);
    UTM_REQUEST.sender.send(UtmRequest::CloneGuest {
        result: tx,
        original_guest_name: original_guest_name.to_owned(),
        new_guest_name: new_guest_name.to_owned(),
    })?;
    Ok(rx.recv()??)
}
