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
    let profile_keys = profile_env_keys(profile);
    for key in profile_keys {
        if let Some(value) = read_nonempty_env(&key) {
            return Some(value);
        }
    }

    read_nonempty_env("PNEUMA_TRANSPORT_PROXY_URL")
}

fn profile_env_keys(profile: &TransportStealthProfile) -> Vec<String> {
    match profile {
        TransportStealthProfile::Chrome(version) => vec![
            format!("PNEUMA_TRANSPORT_PROXY_CHROME_{version}"),
            format!("PNEUMA_TRANSPORT_PROXY_CHROME{version}"), // legacy
            "PNEUMA_TRANSPORT_PROXY_CHROME".to_string(),
        ],
        TransportStealthProfile::Firefox(version) => vec![
            format!("PNEUMA_TRANSPORT_PROXY_FIREFOX_{version}"),
            format!("PNEUMA_TRANSPORT_PROXY_FIREFOX{version}"), // legacy
            "PNEUMA_TRANSPORT_PROXY_FIREFOX".to_string(),
        ],
        TransportStealthProfile::Safari(version) => vec![
            format!("PNEUMA_TRANSPORT_PROXY_SAFARI_{version}"),
            format!("PNEUMA_TRANSPORT_PROXY_SAFARI{version}"), // legacy
            "PNEUMA_TRANSPORT_PROXY_SAFARI".to_string(),
        ],
        TransportStealthProfile::Edge(version) => vec![
            format!("PNEUMA_TRANSPORT_PROXY_EDGE_{version}"),
            format!("PNEUMA_TRANSPORT_PROXY_EDGE{version}"), // legacy
            "PNEUMA_TRANSPORT_PROXY_EDGE".to_string(),
        ],
        TransportStealthProfile::Custom { .. } => {
            vec!["PNEUMA_TRANSPORT_PROXY_CUSTOM".to_string()]
        }
    }
}

fn read_nonempty_env(key: &str) -> Option<String> {
    let value = std::env::var(key).ok()?;
    if value.trim().is_empty() {
        return None;
    }
    Some(value)
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

    #[test]
    fn profile_env_keys_include_version_and_generic() {
        let keys = profile_env_keys(&TransportStealthProfile::Chrome(120));
        assert_eq!(keys[0], "PNEUMA_TRANSPORT_PROXY_CHROME_120");
        assert_eq!(keys[1], "PNEUMA_TRANSPORT_PROXY_CHROME120");
        assert_eq!(keys[2], "PNEUMA_TRANSPORT_PROXY_CHROME");
    }
}
