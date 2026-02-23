use async_trait::async_trait;

use crate::{EngineKind, HeadlessEngine, MigrationEnvelope};

#[cfg(feature = "ladybird")]
use pneuma_ladybird_shim::{evaluate as shim_evaluate, navigate as shim_navigate, LadybirdHandle};

pub struct LadybirdEngine {
    #[cfg(feature = "ladybird")]
    handle: LadybirdHandle,
}

impl std::fmt::Debug for LadybirdEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LadybirdEngine")
    }
}

impl LadybirdEngine {
    #[cfg(feature = "ladybird")]
    pub fn launch() -> anyhow::Result<Self> {
        Ok(Self {
            handle: pneuma_ladybird_shim::launch()?,
        })
    }

    #[cfg(not(feature = "ladybird"))]
    pub fn launch() -> anyhow::Result<Self> {
        anyhow::bail!("LadybirdEngine requires the ladybird feature")
    }
}

#[async_trait]
impl HeadlessEngine for LadybirdEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::Ladybird
    }

    fn name(&self) -> &'static str {
        "ladybird"
    }

    async fn navigate(&self, url: &str, _opts_json: &str) -> anyhow::Result<String> {
        let _ = url;
        #[cfg(feature = "ladybird")]
        {
            let title = shim_navigate(&self.handle, url.to_string()).await?;
            return Ok(format!(
                r#"{{"ok":true,"engine":"ladybird","title":{title_json},"url":{url_json},"dom_count":0,"paint_time_ms":0,"js_errors":0}}"#,
                title_json = serde_json::to_string(&title)?,
                url_json = serde_json::to_string(url)?,
            ));
        }
        #[cfg(not(feature = "ladybird"))]
        anyhow::bail!("LadybirdEngine::navigate requires the ladybird feature")
    }

    async fn evaluate(&self, script: &str) -> anyhow::Result<String> {
        let _ = script;
        #[cfg(feature = "ladybird")]
        {
            return shim_evaluate(&self.handle, script.to_string()).await;
        }
        #[cfg(not(feature = "ladybird"))]
        anyhow::bail!("LadybirdEngine::evaluate requires the ladybird feature")
    }

    async fn screenshot(&self) -> anyhow::Result<Vec<u8>> {
        anyhow::bail!("LadybirdEngine::screenshot not wired yet")
    }

    async fn set_viewport(&self, _width: u32, _height: u32) -> anyhow::Result<()> {
        anyhow::bail!("LadybirdEngine::set_viewport not wired yet")
    }

    async fn get_viewport(&self) -> anyhow::Result<(u32, u32)> {
        anyhow::bail!("LadybirdEngine::get_viewport not wired yet")
    }

    async fn close(&self) -> anyhow::Result<()> {
        anyhow::bail!("LadybirdEngine::close not wired yet")
    }

    async fn extract_state(&self) -> anyhow::Result<MigrationEnvelope> {
        anyhow::bail!("LadybirdEngine::extract_state not wired yet")
    }

    async fn import_state(&self, _state: MigrationEnvelope) -> anyhow::Result<()> {
        anyhow::bail!("LadybirdEngine::import_state not wired yet")
    }
}
