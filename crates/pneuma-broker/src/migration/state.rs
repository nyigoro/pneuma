use pneuma_engines::EngineKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigratableSessionState {
    pub session_id: String,
    pub active_engine: EngineKind,
    pub last_url: Option<String>,
}
