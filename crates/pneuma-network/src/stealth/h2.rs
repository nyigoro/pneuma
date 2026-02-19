use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Http2SpoofProfile {
    pub settings_order: Vec<String>,
    pub window_size: u32,
}
