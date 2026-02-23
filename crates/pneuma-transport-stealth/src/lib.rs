use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pneuma_engines::{ProxyConfig, TransportProvider, TransportStealthProfile};

#[derive(Debug, Clone, Default)]
pub struct LocalProxyTransportProvider {
    cache: Arc<Mutex<HashMap<TransportStealthProfile, ProxyConfig>>>,
}

impl LocalProxyTransportProvider {
    pub fn new() -> Self {
        Self::default()
    }

    fn resolve_profile(&self, profile: &TransportStealthProfile) -> Option<ProxyConfig> {
        let endpoint_raw = resolve_endpoint_from_env(profile)?;
        let endpoint = normalize_proxy_endpoint(&endpoint_raw)?;
        let no_proxy = std::env::var("PNEUMA_TRANSPORT_NO_PROXY")
            .ok()
            .map(|v| parse_csv_list(&v))
            .unwrap_or_default();

        Some(ProxyConfig {
            http_proxy: endpoint.clone(),
            ssl_proxy: Some(endpoint),
            no_proxy,
        })
    }
}

impl TransportProvider for LocalProxyTransportProvider {
    fn proxy_for_profile(&self, profile: &TransportStealthProfile) -> Option<ProxyConfig> {
        if let Ok(guard) = self.cache.lock() {
            if let Some(cached) = guard.get(profile) {
                return Some(cached.clone());
            }
        }

        let resolved = self.resolve_profile(profile)?;
        tracing::info!(
            target: "pneuma_transport_stealth",
            profile = ?profile,
            proxy = %resolved.http_proxy,
            "resolved transport stealth proxy"
        );

        if let Ok(mut guard) = self.cache.lock() {
            guard
                .entry(profile.clone())
                .or_insert_with(|| resolved.clone());
            return guard.get(profile).cloned();
        }

        Some(resolved)
    }
}

fn resolve_endpoint_from_env(profile: &TransportStealthProfile) -> Option<String> {
    let key = match profile {
        TransportStealthProfile::Chrome120 => Some("PNEUMA_TRANSPORT_PROXY_CHROME120"),
        TransportStealthProfile::Safari17 => Some("PNEUMA_TRANSPORT_PROXY_SAFARI17"),
        TransportStealthProfile::Firefox123 => Some("PNEUMA_TRANSPORT_PROXY_FIREFOX123"),
        TransportStealthProfile::Custom { .. } => Some("PNEUMA_TRANSPORT_PROXY_CUSTOM"),
    };

    key.and_then(|k| std::env::var(k).ok())
        .or_else(|| std::env::var("PNEUMA_TRANSPORT_PROXY_URL").ok())
}

fn normalize_proxy_endpoint(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((_, rest)) = trimmed.split_once("://") {
        let authority = rest.split('/').next().unwrap_or(rest).trim();
        if authority.is_empty() {
            return None;
        }
        return Some(authority.to_string());
    }

    Some(trimmed.to_string())
}

fn parse_csv_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_proxy_endpoint_removes_scheme_and_path() {
        assert_eq!(
            normalize_proxy_endpoint("http://127.0.0.1:8080/path"),
            Some("127.0.0.1:8080".to_string())
        );
    }

    #[test]
    fn parse_csv_list_filters_empty_entries() {
        assert_eq!(
            parse_csv_list(" localhost, 127.0.0.1, ,example.com "),
            vec!["localhost", "127.0.0.1", "example.com"]
        );
    }
}
