use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserIdentity {
    pub name: String,
    pub user_agent: String,
    pub accept_language: String,
}

impl Default for BrowserIdentity {
    fn default() -> Self {
        Self {
            name: "chrome-120-windows".to_string(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36".to_string(),
            accept_language: "en-US,en;q=0.9".to_string(),
        }
    }
}
