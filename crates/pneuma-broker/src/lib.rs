pub mod broker;
pub mod confidence;
pub mod engine_factory;
pub mod handle;
pub mod migration;
pub mod service;

use pneuma_engines::{EngineKind, TransportStealthProfile};

pub use broker::Broker;
pub use handle::{BrokerHandle, BrokerRequest};

#[derive(Debug, Clone)]
pub struct LaunchTemplate {
    pub kind: EngineKind,
    pub stealth: bool,
    pub initial_transport: Option<TransportStealthProfile>,
}
