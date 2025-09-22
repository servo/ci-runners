use serde::{Deserialize, Serialize};

use crate::{TOML, units::MemorySize};

#[derive(Clone, Debug, Deserialize)]
pub struct Profile {
    pub profile_name: String,
    pub configuration_name: String,
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
    pub fn profile_guest_name(&self) -> String {
        format!("{}", self.profile_name)
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
