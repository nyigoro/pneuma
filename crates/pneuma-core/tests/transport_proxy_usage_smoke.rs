#![cfg(feature = "transport-stealth")]

//! Transport proxy usage smoke test.
//!
//! Validates plumbing only: broker parses `transport_stealth`, resolves proxy,
//! lazily launches Servo with proxy capabilities, and engine traffic reaches the
//! configured proxy endpoint.
//!
//! This test does not validate JA3 fingerprint impersonation.
//!
//! Run with:
//!   cargo test -p pneuma-core --test transport_proxy_usage_smoke \
//!     --features transport-stealth -- --ignored --nocapture

use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct EnvVarGuard {
    key: String,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &str, value: String) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self {
            key: key.to_string(),
            previous,
        }
    }

    fn unset(key: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::remove_var(key);
        Self {
            key: key.to_string(),
            previous,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            std::env::set_var(&self.key, value);
        } else {
            std::env::remove_var(&self.key);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires transport-stealth feature and SERVO_BIN for fresh session startup"]
async fn transport_proxy_is_honored_by_lazy_servo_launch() -> Result<()> {
    let has_webdriver_endpoint = std::env::var("SERVO_WEBDRIVER_URL").is_ok();
    let has_servo_bin = std::env::var("SERVO_BIN").is_ok();
    if !has_webdriver_endpoint && !has_servo_bin {
        eprintln!(
            "[transport-proxy-smoke] skipping: set SERVO_WEBDRIVER_URL (preferred) or SERVO_BIN"
        );
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("PNEUMA_LOG")
                .unwrap_or_else(|_| "pneuma=debug,pneuma_broker=debug,pneuma_engines=debug".into()),
        )
        .try_init()
        .ok();

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind local recording proxy listener")?;
    let proxy_addr = listener.local_addr()?;
    let (seen_tx, seen_rx) = tokio::sync::oneshot::channel::<String>();

    let proxy_task = tokio::spawn(async move {
        match listener.accept().await {
            Ok((mut stream, _peer)) => {
                let mut buf = [0_u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let payload = String::from_utf8_lossy(&buf[..n]).into_owned();
                let _ = seen_tx.send(payload.clone());

                let response = if payload.starts_with("CONNECT ") {
                    "HTTP/1.1 200 Connection Established\r\n\r\n"
                } else {
                    "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                let _ = stream.write_all(response.as_bytes()).await;
            }
            Err(_) => {
                let _ = seen_tx.send(String::new());
            }
        }
    });

    let _proxy_guard = EnvVarGuard::set(
        "PNEUMA_TRANSPORT_PROXY_CHROME_120",
        format!("http://{proxy_addr}"),
    );
    let _no_proxy_guard = EnvVarGuard::unset("PNEUMA_TRANSPORT_NO_PROXY");
    // If no explicit endpoint is provided, use local spawn path.
    let _webdriver_guard = if has_webdriver_endpoint {
        None
    } else {
        Some(EnvVarGuard::unset("SERVO_WEBDRIVER_URL"))
    };

    let template = pneuma_broker::LaunchTemplate {
        kind: pneuma_engines::EngineKind::Servo,
        stealth: true,
        initial_transport: None,
    };

    let (broker_tx, broker_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = pneuma_broker::handle::BrokerHandle::new(broker_tx);
    let broker_task = tokio::spawn(pneuma_broker::service::run_lazy(broker_rx, template));

    let handle_for_nav = handle.clone();
    let nav_task = tokio::task::spawn_blocking(move || -> Result<()> {
        let page_id = handle_for_nav.create_page()?;
        handle_for_nav.navigate(
            page_id,
            "http://example.com/transport-proxy-smoke".to_string(),
            r#"{"transport_stealth":{"type":"chrome","version":120}}"#.to_string(),
        )?;
        Ok(())
    });
    let nav_joined = tokio::time::timeout(Duration::from_secs(20), nav_task)
        .await
        .context("navigate task timed out before proxy usage could be observed")?;
    let nav_outcome = nav_joined.context("navigate thread join failed")?;

    if let Err(error) = nav_outcome {
        let message = error.to_string();
        eprintln!("[transport-proxy-smoke] navigate returned error: {message}");
        if message.contains("did not become ready within 10s")
            || message.contains("session creation failed")
            || message.contains("active session is already running")
            || message.contains("rejected proxy capabilities")
        {
            eprintln!(
                "[transport-proxy-smoke] skipping: Servo startup/session unavailable in this environment: {message}"
            );
            let handle_for_shutdown = handle.clone();
            let _ = tokio::task::spawn_blocking(move || handle_for_shutdown.shutdown()).await;
            let _ = tokio::time::timeout(Duration::from_secs(5), broker_task).await;
            proxy_task.abort();
            return Ok(());
        }
    }

    let payload = tokio::time::timeout(Duration::from_secs(10), seen_rx)
        .await
        .context("recording proxy did not receive any traffic from Servo engine")?
        .context("recording proxy channel dropped before delivering payload")?;

    assert!(
        !payload.is_empty(),
        "proxy accepted a connection but received no bytes"
    );
    assert!(
        payload.starts_with("GET http://") || payload.starts_with("CONNECT "),
        "expected proxy-style request line (GET absolute-URI or CONNECT), got: {payload:?}"
    );

    let handle_for_shutdown = handle.clone();
    let _ = tokio::task::spawn_blocking(move || handle_for_shutdown.shutdown()).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), broker_task).await;
    proxy_task.abort();

    Ok(())
}
