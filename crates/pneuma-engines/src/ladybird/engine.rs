use async_trait::async_trait;

use crate::{EngineKind, HeadlessEngine};

#[derive(Debug, Default)]
pub struct LadybirdEngine;

#[async_trait]
impl HeadlessEngine for LadybirdEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::Ladybird
    }

    fn name(&self) -> &'static str {
        "ladybird"
    }

    async fn navigate(&self, _url: &str, _opts_json: &str) -> anyhow::Result<String> {
        anyhow::bail!("ladybird engine is not wired yet")
    }

    async fn evaluate(&self, _script: &str) -> anyhow::Result<String> {
        anyhow::bail!("ladybird engine is not wired yet")
    }

    async fn screenshot(&self) -> anyhow::Result<Vec<u8>> {
        anyhow::bail!("ladybird engine is not wired yet")
    }

    async fn close(&self) -> anyhow::Result<()> {
        anyhow::bail!("ladybird engine is not wired yet")
    }
}
