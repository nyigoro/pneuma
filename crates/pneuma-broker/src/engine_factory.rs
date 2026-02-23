use anyhow::Result;
use async_trait::async_trait;
use pneuma_engines::{EngineKind, HeadlessEngine};

/// Abstraction over secondary engine creation, primarily for testability.
///
/// The `target` argument reflects the decision the confidence scorer made
/// (e.g. `EngineKind::Ladybird`). When the `ladybird` feature is enabled and
/// `LADYBIRD_BUILD_DIR` is set, `EngineKind::Ladybird` routes to
/// `LadybirdEngine`; otherwise factory falls back to a secondary Servo proxy.
#[async_trait]
pub trait EscalationEngineFactory: Send + Sync {
    async fn create_for_escalation(&self, target: EngineKind) -> Result<Box<dyn HeadlessEngine>>;
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
        match target {
            EngineKind::Ladybird => {
                #[cfg(feature = "ladybird")]
                if std::env::var_os("LADYBIRD_BUILD_DIR").is_some() {
                    tracing::info!(
                        target: "pneuma_broker",
                        "escalation target is Ladybird; LADYBIRD_BUILD_DIR set, launching Ladybird secondary"
                    );
                    let engine = pneuma_engines::ladybird::LadybirdEngine::launch()?;
                    return Ok(Box::new(engine));
                }

                tracing::info!(
                    target: "pneuma_broker",
                    "escalation target is Ladybird; using secondary Servo proxy fallback"
                );
            }
            EngineKind::Servo => {
                tracing::info!(
                    target: "pneuma_broker",
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
                    "escalation factory: attaching to SERVO_SECONDARY_WEBDRIVER_URL"
                );
                let engine = pneuma_engines::servo::ServoEngine::launch_with_endpoint(trimmed).await?;
                return Ok(Box::new(engine));
            }
        }

        tracing::info!(
            target: "pneuma_broker",
            "escalation factory: no endpoint env var set; spawning local Servo process for secondary"
        );
        let engine = pneuma_engines::servo::ServoEngine::launch_spawned().await?;
        Ok(Box::new(engine))
    }
}
