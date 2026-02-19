use crate::{EngineKind, HeadlessEngine};

#[derive(Debug, Default)]
pub struct LadybirdEngine;

impl HeadlessEngine for LadybirdEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::Ladybird
    }

    fn name(&self) -> &'static str {
        "ladybird"
    }
}
