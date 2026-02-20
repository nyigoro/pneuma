use anyhow::Result;
use async_trait::async_trait;
use pneuma_engines::{EngineKind, HeadlessEngine};

/// Abstraction over secondary engine creation, primarily for testability.
///
/// The `target` argument reflects the decision the confidence scorer made
/// (e.g. `EngineKind::Ladybird`). In Week 10, `Ladybird` is not yet wired, so
/// all targets map to a secondary Servo proxy. A real Ladybird factory can be
/// dropped in later without touching service.rs.
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
        // Ladybird is not wired yet in Week 10. We proxy all escalation targets
        // through a secondary Servo instance. This is the explicit temporary
        // mapping described in the spec.
        match target {
            EngineKind::Ladybird => {
                tracing::info!(
                    target: "pneuma_broker",
                    "escalation target is Ladybird; using secondary Servo proxy (Week 10 temporary mapping)"
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
