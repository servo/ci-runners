use serde::{Deserialize, Serialize};

use crate::units::MemorySize;

#[derive(Clone, Debug, Deserialize)]
pub struct Profile {
    pub configuration_name: String,
    pub base_vm_name: String,
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
