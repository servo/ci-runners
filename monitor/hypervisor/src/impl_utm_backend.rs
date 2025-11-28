use std::{process::Command, thread::sleep, time::Duration};

use jane_eyre::eyre;
use osakit::{self, declare_script};
use settings::TOML;
use tracing::{error, warn};

declare_script! {
    #[language(JavaScript)]
    #[source(r#"
        // Like `Array.from()`, but works on array-like objects created by applications
        // (which would otherwise throw “Error: Error: Can't get object.”).
        function array(xs) {
            const result = [];
            for (var i in xs) {
                result.push(xs[i]);
            }
            return result;
        }
        function list_guests() {
            const utm = Application("UTM");
            const vms = array(utm.virtualMachines);
            return vms.map(vm => vm.name());
        }
        function start_guest(guest_name) {
            const utm = Application("UTM");
            const vm = array(utm.virtualMachines).find(vm => vm.name() == guest_name);
            vm.start();
        }
        function delete_guest(guest_name) {
            const utm = Application("UTM");
            const vm = array(utm.virtualMachines).find(vm => vm.name() == guest_name);
            if (vm) {
                vm.delete();
            }
        }
        function guest_status(guest_name) {
            const utm = Application("UTM");
            const vm = array(utm.virtualMachines).find(vm => vm.name() == guest_name);
            return vm.status();
        }
    "#)]
    Script {
        fn list_guests() -> Vec<String>;
        fn start_guest(guest_name: &str);
        fn delete_guest(guest_name: &str);
        fn guest_status(guest_name: &str) -> String;
    }
}

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
        error!("Either UTM is not installed, someone chose to deny the permission,");
        error!("or you are running the monitor with sudo (try without sudo).");
        error!("If UTM is installed, try clearing the automation permissions list:");
        // <https://apple.stackexchange.com/a/360610>
        error!("$ tccutil reset AppleEvents");
        panic!("Failed to acquire permission");
    }
    Ok(())
}

pub fn list_guests() -> eyre::Result<Vec<String>> {
    // Output is not filtered by prefix, so we must filter it ourselves.
    let prefix = format!("{}-", TOML.libvirt_runner_guest_prefix());
    let result = Script::new()?
        .list_guests()?
        .into_iter()
        .filter(|name| name.starts_with(&prefix));

    Ok(result.collect())
}

pub fn guest_status(guest_name: &str) -> eyre::Result<String> {
    Ok(Script::new()?.guest_status(guest_name)?)
}

pub fn start_guest(guest_name: &str) -> eyre::Result<()> {
    Ok(Script::new()?.start_guest(guest_name)?)
}

pub fn delete_guest(guest_name: &str) -> eyre::Result<()> {
    Ok(Script::new()?.delete_guest(guest_name)?)
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

pub fn rename_guest(old_guest_name: &str, new_guest_name: &str) -> eyre::Result<()> {
    declare_script! {
        #[language(AppleScript)]
        #[source(r#"
            on rename_guest(old_guest_name, new_guest_name)
                tell application "UTM"
                    set vm to virtual machine named old_guest_name
                    set config to configuration of vm
                    set name of config to new_guest_name
                    update configuration vm with config
                end tell
            end rename_guest
        "#)]
        Script {
            fn rename_guest(old_guest_name: &str, new_guest_name: &str);
        }
    }
    Script::new()?.rename_guest(old_guest_name, new_guest_name)?;
    Ok(())
}
