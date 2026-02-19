use crate::{EngineKind, HeadlessEngine};

#[derive(Debug, Default)]
pub struct ServoEngine;

impl HeadlessEngine for ServoEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::Servo
    }

    fn name(&self) -> &'static str {
        "servo"
    }
}
