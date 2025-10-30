use std::{process::Command, thread::sleep, time::Duration};

use jane_eyre::eyre;
use osakit::{self, declare_script};
use tracing::{error, warn};

/// Trigger an automation permission prompt for UTM, on behalf of whatever context the monitor
/// is running in (sshd-keygen-wrapper, Terminal, etc).
///
/// Panics if UTM is not installed or someone chose to deny permission.
pub fn request_automation_permission() -> eyre::Result<()> {
    // Not sure why the osakit crate can’t do this.
    let mut child = Command::new("osascript")
        .args([
            "-e",
            r#"tell application "UTM""#,
            "-e",
            "set vms to virtual machines",
            "-e",
            "end tell",
        ])
        .spawn()?;
    sleep(Duration::from_millis(250));
    let mut warned = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if !warned {
            warn!("Waiting for permission prompt; please check the macOS UI");
            warned = true;
        }
        sleep(Duration::from_millis(250));
    };
    if !status.success() {
        error!("Failed to acquire automation permission for UTM!");
        error!("Either UTM is not installed, or someone chose to deny the permission.");
        error!("If UTM is installed, try clearing the automation permissions list:");
        // <https://apple.stackexchange.com/a/360610>
        error!("$ tccutil reset AppleEvents");
        panic!("Failed to acquire permission");
    }
    Ok(())
}

pub fn clone_guest(original_guest_name: &str, new_guest_name: &str) -> eyre::Result<()> {
    declare_script! {
        #[language(AppleScript)]
        #[source(r#"
            on clone_guest(original_guest_name, new_guest_name)
                tell application "UTM"
                    set vm to virtual machine named original_guest_name
                    duplicate vm with properties {configuration: {name: new_guest_name}}
                end tell
            end clone_guest
        "#)]
        Script {
            fn clone_guest(original_guest_name: &str, new_guest_name: &str);
        }
    }
    Script::new()?.clone_guest(original_guest_name, new_guest_name)?;
    Ok(())
}
