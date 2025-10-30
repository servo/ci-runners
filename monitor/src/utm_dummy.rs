use jane_eyre::eyre;

#[allow(dead_code)]
pub fn request_automation_permission() -> eyre::Result<()> {
    unimplemented!(r#"Requires `#[cfg(target_os = "macos")]`"#)
}

#[allow(dead_code)]
pub fn list_runner_guests() -> eyre::Result<Vec<String>> {
    Ok(vec![])
}

#[allow(dead_code)]
pub fn delete_guest(_guest_name: &str) -> eyre::Result<()> {
    unimplemented!(r#"Requires `#[cfg(target_os = "macos")]`"#)
}

#[allow(dead_code)]
pub fn clone_guest(_original_guest_name: &str, _new_guest_name: &str) -> eyre::Result<()> {
    unimplemented!(r#"Requires `#[cfg(target_os = "macos")]`"#)
}
