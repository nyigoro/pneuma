use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsFingerprintProfile {
    pub ja3: String,
    pub ja4: String,
}
