use anyhow::Result;
use async_trait::async_trait;
use pneuma_engines::{EngineKind, HeadlessEngine, ProxyConfig};

/// Abstraction over secondary engine creation, primarily for testability.
///
/// The `target` argument reflects the decision the confidence scorer made
/// (e.g. `EngineKind::Ladybird`). When the `ladybird` feature is enabled and
/// `LADYBIRD_BUILD_DIR` is set, `EngineKind::Ladybird` routes to
/// `LadybirdEngine`; otherwise factory falls back to a secondary Servo proxy.
#[async_trait]
pub trait EscalationEngineFactory: Send + Sync {
    async fn create_for_escalation(&self, target: EngineKind) -> Result<Box<dyn HeadlessEngine>>;

    async fn create_for_escalation_with_transport(
        &self,
        target: EngineKind,
        _transport_proxy: Option<ProxyConfig>,
    ) -> Result<Box<dyn HeadlessEngine>> {
        self.create_for_escalation(target).await
    }
}

/// Default factory used in production.
///
/// Resolution order for the secondary Servo instance:
/// 1. `SERVO_SECONDARY_WEBDRIVER_URL` — attach to existing process.
/// 2. Spawn a fresh local Servo process.
pub struct DefaultEscalationEngineFactory;

#[async_trait]
impl EscalationEngineFactory for DefaultEscalationEngineFactory {
    async fn create_for_escalation(&self, target: EngineKind) -> Result<Box<dyn HeadlessEngine>> {
        self.create_for_escalation_with_transport(target, None)
            .await
    }

    async fn create_for_escalation_with_transport(
        &self,
        target: EngineKind,
        transport_proxy: Option<ProxyConfig>,
    ) -> Result<Box<dyn HeadlessEngine>> {
        match target {
            EngineKind::Ladybird => {
                if transport_proxy.is_some() {
                    tracing::warn!(
                        target: "pneuma_broker",
                        "this Ladybird build does not honor transport proxy in RequestServer yet; using secondary Servo fail-secure fallback"
                    );
                    tracing::info!(
                        target: "pneuma_broker",
                        has_transport_proxy = true,
                        "escalation target is Ladybird; using secondary Servo proxy fallback"
                    );
                }

                #[cfg(feature = "ladybird")]
                if std::env::var_os("LADYBIRD_BUILD_DIR").is_some() && transport_proxy.is_none() {
                    tracing::info!(
                        target: "pneuma_broker",
                        has_transport_proxy = transport_proxy.is_some(),
                        "escalation target is Ladybird; LADYBIRD_BUILD_DIR set, launching Ladybird secondary"
                    );
                    let engine = pneuma_engines::ladybird::LadybirdEngine::launch_with_proxy(
                        transport_proxy.clone(),
                    )?;
                    return Ok(Box::new(engine));
                } else if transport_proxy.is_none() {
                    tracing::info!(
                        target: "pneuma_broker",
                        has_transport_proxy = transport_proxy.is_some(),
                        "escalation target is Ladybird but LADYBIRD_BUILD_DIR is not set; using secondary Servo proxy fallback"
                    );
                }
                tracing::info!(
                    target: "pneuma_broker",
                    has_transport_proxy = transport_proxy.is_some(),
                    "escalation target is Ladybird; using secondary Servo proxy fallback"
                );
            }
            EngineKind::Servo => {
                tracing::info!(
                    target: "pneuma_broker",
                    has_transport_proxy = transport_proxy.is_some(),
                    "escalation factory: creating secondary Servo instance"
                );
            }
        }

        if let Ok(url) = std::env::var("SERVO_SECONDARY_WEBDRIVER_URL") {
            let trimmed = url.trim().to_string();
            if !trimmed.is_empty() {
                tracing::info!(
                    target: "pneuma_broker",
                    base_url = %trimmed,
                    has_transport_proxy = transport_proxy.is_some(),
                    "escalation factory: attaching to SERVO_SECONDARY_WEBDRIVER_URL"
                );
                let engine = pneuma_engines::servo::ServoEngine::launch_with_endpoint_and_proxy(
                    trimmed,
                    transport_proxy,
                )
                .await?;
                return Ok(Box::new(engine));
            }
        }

        tracing::info!(
            target: "pneuma_broker",
            has_transport_proxy = transport_proxy.is_some(),
            "escalation factory: no endpoint env var set; spawning local Servo process for secondary"
        );
        let engine =
            pneuma_engines::servo::ServoEngine::launch_spawned_with_proxy(transport_proxy).await?;
        Ok(Box::new(engine))
    }
}
