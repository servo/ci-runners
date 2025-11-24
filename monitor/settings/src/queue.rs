use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct QueueConfig {
    pub servers: Vec<String>,
}
