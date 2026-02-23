use crate::ProxyConfig;

pub fn proxy_server_value(proxy_config: &ProxyConfig) -> String {
    proxy_config.ssl_or_http().to_string()
}

/// Build launch arguments for transport proxy routing.
///
/// Ladybird transport proxy wiring is not active yet, but this helper keeps
/// argument-shaping logic isolated from `engine.rs`.
pub fn build_proxy_args(proxy_config: &ProxyConfig) -> Vec<String> {
    let mut args = vec![format!("--proxy-server={}", proxy_server_value(proxy_config))];
    if !proxy_config.no_proxy.is_empty() {
        args.push(format!(
            "--proxy-bypass-list={}",
            proxy_config.no_proxy.join(",")
        ));
    }
    args
}

#[cfg(test)]
mod tests {
    use super::{build_proxy_args, proxy_server_value};
    use crate::ProxyConfig;

    #[test]
    fn builds_proxy_cli_args() {
        let cfg = ProxyConfig {
            http_proxy: "127.0.0.1:8888".into(),
            ssl_proxy: Some("127.0.0.1:9999".into()),
            no_proxy: vec!["localhost".into(), "127.0.0.1".into()],
        };
        let args = build_proxy_args(&cfg);
        assert_eq!(args[0], "--proxy-server=127.0.0.1:9999");
        assert_eq!(args[1], "--proxy-bypass-list=localhost,127.0.0.1");
    }

    #[test]
    fn proxy_server_prefers_ssl_proxy() {
        let cfg = ProxyConfig {
            http_proxy: "127.0.0.1:8888".into(),
            ssl_proxy: Some("127.0.0.1:9999".into()),
            no_proxy: vec![],
        };
        assert_eq!(proxy_server_value(&cfg), "127.0.0.1:9999");
    }
}
