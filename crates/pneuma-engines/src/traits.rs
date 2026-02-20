use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::migration::MigrationEnvelope;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    Servo,
    Ladybird,
}

impl std::fmt::Display for EngineKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            EngineKind::Servo => "servo",
            EngineKind::Ladybird => "ladybird",
        };
        write!(f, "{label}")
    }
}

#[async_trait]
pub trait HeadlessEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn name(&self) -> &'static str;
    async fn navigate(&self, url: &str, opts_json: &str) -> anyhow::Result<String>;
    async fn evaluate(&self, script: &str) -> anyhow::Result<String>;
    async fn screenshot(&self) -> anyhow::Result<Vec<u8>>;
    async fn close(&self) -> anyhow::Result<()>;

    /// Capture cookies and current-origin localStorage into a portable envelope.
    ///
    /// Implementations should make a best-effort capture; partial results are
    /// acceptable. An `Err` return means capture failed entirely and escalation
    /// should fall back to the primary result.
    async fn extract_state(&self) -> anyhow::Result<MigrationEnvelope>;

    /// Restore state from a [`MigrationEnvelope`] into this engine instance.
    ///
    /// The engine must already be on a page in the target origin before
    /// `import_state` is called (so that cookie domain and localStorage context
    /// are valid). Partial import failures are logged but do not cause an `Err`
    /// return unless the whole operation is unrecoverable.
    async fn import_state(&self, state: MigrationEnvelope) -> anyhow::Result<()>;
}
