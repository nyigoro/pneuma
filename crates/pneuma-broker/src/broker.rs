use anyhow::Result;
use pneuma_engines::EngineKind;

use crate::confidence::{ConfidenceScorer, ConfidenceSignals, EngineDecision};

#[derive(Debug)]
pub struct Broker {
    engine: EngineKind,
    stealth: bool,
    scorer: ConfidenceScorer,
}

impl Broker {
    pub fn new(engine: EngineKind, stealth: bool) -> Result<Self> {
        Ok(Self {
            engine,
            stealth,
            scorer: ConfidenceScorer::default(),
        })
    }

    pub fn route(&self, signals: &ConfidenceSignals) -> EngineKind {
        let report = self.scorer.score(signals);
        if self.stealth && matches!(report.decision, EngineDecision::EscalateToLadybird(_)) {
            EngineKind::Ladybird
        } else {
            self.engine
        }
    }
}
