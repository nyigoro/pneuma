use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceSignals {
    pub base_score: f32,
    pub network_failures: u32,
    pub js_errors: u32,
}

impl Default for ConfidenceSignals {
    fn default() -> Self {
        Self {
            base_score: 1.0,
            network_failures: 0,
            js_errors: 0,
        }
    }
}
