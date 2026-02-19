use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    Servo,
    Ladybird,
}

impl std::fmt::Display for EngineKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            EngineKind::Servo => "servo",
            EngineKind::Ladybird => "ladybird",
        };
        write!(f, "{label}")
    }
}

pub trait HeadlessEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn name(&self) -> &'static str;
}
