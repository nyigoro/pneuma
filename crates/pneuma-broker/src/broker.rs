use anyhow::Result;
use pneuma_engines::EngineKind;

use crate::confidence::{ConfidenceScorer, ConfidenceSignals};

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
        if self.stealth && self.scorer.score(signals) < 0.5 {
            EngineKind::Ladybird
        } else {
            self.engine
        }
    }
}
