use async_trait::async_trait;
#[cfg(feature = "ladybird")]
use anyhow::Context;

#[cfg(feature = "ladybird")]
use crate::ladybird::stealth::proxy_server_value;
use crate::{EngineKind, HeadlessEngine, MigrationEnvelope, ProxyConfig};

#[cfg(feature = "ladybird")]
use pneuma_ladybird_shim::{
    evaluate as shim_evaluate,
    get_viewport as shim_get_viewport,
    launch_with_proxy as shim_launch_with_proxy,
    navigate as shim_navigate,
    screenshot as shim_screenshot,
    set_viewport as shim_set_viewport,
    LadybirdHandle,
};

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
    pub fn launch_with_proxy(proxy: Option<ProxyConfig>) -> anyhow::Result<Self> {
        let proxy_server = proxy.as_ref().map(proxy_server_value);
        Ok(Self {
            handle: shim_launch_with_proxy(proxy_server)?,
        })
    }

    #[cfg(not(feature = "ladybird"))]
    pub fn launch_with_proxy(proxy: Option<ProxyConfig>) -> anyhow::Result<Self> {
        let _ = proxy;
        anyhow::bail!("LadybirdEngine requires the ladybird feature")
    }

    #[cfg(feature = "ladybird")]
    pub fn launch() -> anyhow::Result<Self> {
        Self::launch_with_proxy(None)
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
        #[cfg(feature = "ladybird")]
        {
            let path = shim_screenshot(&self.handle, false).await?;
            let bytes = tokio::fs::read(&path)
                .await
                .with_context(|| format!("failed to read Ladybird screenshot at {path}"))?;
            return Ok(bytes);
        }
        #[cfg(not(feature = "ladybird"))]
        anyhow::bail!("LadybirdEngine::screenshot requires the ladybird feature")
    }

    async fn set_viewport(&self, _width: u32, _height: u32) -> anyhow::Result<()> {
        #[cfg(feature = "ladybird")]
        {
            return shim_set_viewport(&self.handle, _width, _height).await;
        }
        #[cfg(not(feature = "ladybird"))]
        anyhow::bail!("LadybirdEngine::set_viewport requires the ladybird feature")
    }

    async fn get_viewport(&self) -> anyhow::Result<(u32, u32)> {
        #[cfg(feature = "ladybird")]
        {
            return shim_get_viewport(&self.handle).await;
        }
        #[cfg(not(feature = "ladybird"))]
        anyhow::bail!("LadybirdEngine::get_viewport requires the ladybird feature")
    }

    async fn close(&self) -> anyhow::Result<()> {
        anyhow::bail!("LadybirdEngine::close not wired yet")
    }

    async fn extract_state(&self) -> anyhow::Result<MigrationEnvelope> {
        anyhow::bail!("LadybirdEngine::extract_state not wired yet")
    }

    async fn import_state(&self, _state: MigrationEnvelope) -> anyhow::Result<()> {
        // Ladybird does not support session state migration in this milestone.
        // Accept and discard the envelope so escalation can proceed with a
        // clean browser context.
        Ok(())
    }
}
