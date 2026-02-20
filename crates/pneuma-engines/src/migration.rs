use serde::{Deserialize, Serialize};

use crate::EngineKind;

/// A portable snapshot of browser state that can be transferred between engine instances.
///
/// Week 10 scope: cookies + current-origin localStorage only.
/// Network/IndexedDB/sessionStorage migration is deferred to a later week.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationEnvelope {
    /// Engine that produced this snapshot.
    pub source_engine: EngineKind,
    /// Unix timestamp in milliseconds at capture time.
    pub captured_at_ms: u64,
    /// URL the engine was on when the snapshot was taken.
    pub current_url: Option<String>,
    /// Cookies visible to the WebDriver session at capture time.
    pub cookies: Vec<MigrationCookie>,
    /// Current-origin localStorage key/value pairs.
    pub local_storage: Vec<LocalStorageEntry>,
}

/// A single cookie transferred across engine instances.
///
/// Fields mirror the WebDriver cookie object (W3C §14.1).
/// Optional fields are omitted when the engine does not populate them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationCookie {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_only: Option<bool>,
    /// Expiry as Unix timestamp seconds (WebDriver `expiry` field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_site: Option<String>,
}

/// A single localStorage key/value pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalStorageEntry {
    pub key: String,
    pub value: String,
}
