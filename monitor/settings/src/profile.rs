use jane_eyre::eyre::{self, OptionExt};
use serde::{Deserialize, Serialize};

use crate::{TOML, units::MemorySize};

#[derive(Clone, Debug, Deserialize)]
pub struct Profile {
    pub profile_name: String,
    pub github_runner_label: String,
    pub target_count: usize,
    #[serde(default)]
    pub image_type: ImageType,
    pub requires_1g_hugepages: usize,
    pub requires_normal_memory: MemorySize,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub enum ImageType {
    #[default]
    Rust,
}

impl Profile {
    pub fn snapshot_path_slug(&self, snapshot_name: &str) -> String {
        format!("{}@{snapshot_name}", self.profile_name)
    }

    pub fn template_guest_name(&self, snapshot_name: &str) -> String {
        format!(
            "{}-{}@{snapshot_name}",
            TOML.libvirt_template_guest_prefix(),
            self.profile_name
        )
    }

    pub fn rebuild_guest_name(&self, snapshot_name: &str) -> String {
        format!(
            "{}-{}@{snapshot_name}",
            TOML.libvirt_rebuild_guest_prefix(),
            self.profile_name
        )
    }

    pub fn runner_guest_name(&self, id: usize) -> String {
        format!(
            "{}-{}.{}",
            TOML.libvirt_runner_guest_prefix(),
            self.profile_name,
            id
        )
    }
}

pub fn parse_template_guest_name(template_guest_name: &str) -> eyre::Result<(&str, &str)> {
    let prefix = format!("{}-", TOML.libvirt_template_guest_prefix());
    let (profile_key, snapshot_name) = template_guest_name
        .strip_prefix(&prefix)
        .ok_or_eyre("Failed to strip template guest prefix")?
        .split_once("@")
        .ok_or_eyre("Failed to split snapshot path slug into profile key and snapshot name")?;
    Ok((profile_key, snapshot_name))
}

pub fn parse_rebuild_guest_name(rebuild_guest_name: &str) -> eyre::Result<(&str, &str)> {
    let prefix = format!("{}-", TOML.libvirt_rebuild_guest_prefix());
    let (profile_key, snapshot_name) = rebuild_guest_name
        .strip_prefix(&prefix)
        .ok_or_eyre("Failed to strip rebuild guest prefix")?
        .split_once("@")
        .ok_or_eyre("Failed to split snapshot path slug into profile key and snapshot name")?;
    Ok((profile_key, snapshot_name))
}
