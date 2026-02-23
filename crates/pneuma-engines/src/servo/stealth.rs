use serde_json::Value;

use crate::ProxyConfig;

pub(crate) fn build_proxy_capabilities(proxy_config: &ProxyConfig) -> Value {
    let mut proxy = serde_json::Map::new();
    proxy.insert("proxyType".into(), Value::String("manual".into()));
    proxy.insert(
        "httpProxy".into(),
        Value::String(proxy_config.http_proxy.clone()),
    );
    proxy.insert(
        "sslProxy".into(),
        Value::String(proxy_config.ssl_or_http().to_string()),
    );
    if !proxy_config.no_proxy.is_empty() {
        proxy.insert(
            "noProxy".into(),
            Value::String(proxy_config.no_proxy.join(",")),
        );
    }
    Value::Object(proxy)
}

#[cfg(test)]
mod tests {
    use super::build_proxy_capabilities;
    use crate::ProxyConfig;

    #[test]
    fn builds_manual_proxy_capabilities() {
        let cfg = ProxyConfig {
            http_proxy: "127.0.0.1:8080".into(),
            ssl_proxy: Some("127.0.0.1:8443".into()),
            no_proxy: vec!["localhost".into(), "127.0.0.1".into()],
        };
        let caps = build_proxy_capabilities(&cfg);
        let proxy = caps.as_object().expect("proxy capabilities must be object");

        assert_eq!(
            proxy.get("proxyType").and_then(|v| v.as_str()),
            Some("manual")
        );
        assert_eq!(
            proxy.get("httpProxy").and_then(|v| v.as_str()),
            Some("127.0.0.1:8080")
        );
        assert_eq!(
            proxy.get("sslProxy").and_then(|v| v.as_str()),
            Some("127.0.0.1:8443")
        );
        assert_eq!(
            proxy.get("noProxy").and_then(|v| v.as_str()),
            Some("localhost,127.0.0.1")
        );
    }
}
