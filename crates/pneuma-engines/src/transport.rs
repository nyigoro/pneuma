use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransportStealthProfile {
    Chrome120,
    Safari17,
    Firefox123,
    Custom {
        ja3: String,
        h2_settings: Vec<u8>,
        alpn: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// WebDriver manual proxy value in host:port form.
    pub http_proxy: String,
    /// Optional HTTPS proxy endpoint in host:port form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_proxy: Option<String>,
    /// Optional comma-separated bypass list expressed as entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_proxy: Vec<String>,
}

impl ProxyConfig {
    pub fn ssl_or_http(&self) -> &str {
        self.ssl_proxy.as_deref().unwrap_or(&self.http_proxy)
    }
}

pub trait TransportProvider: Send + Sync {
    fn proxy_for_profile(&self, profile: &TransportStealthProfile) -> Option<ProxyConfig>;
}
