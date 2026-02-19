use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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
}
