//! Week 12 escalation smoke test.
//!
//! Requires two live Servo WebDriver endpoints:
//!   SERVO_WEBDRIVER_URL           -- primary
//!   SERVO_SECONDARY_WEBDRIVER_URL -- secondary
//!
//! Run manually with:
//!   cargo test -p pneuma-core --test week12_escalation_smoke -- --ignored --nocapture

use anyhow::Result;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const DATA_FIXTURE_URL: &str =
    "data:text/html;base64,PCFET0NUWVBFIGh0bWw+PGh0bWw+PGhlYWQ+PC9oZWFkPjxib2R5PjwvYm9keT48L2h0bWw+";

/// Minimal HTTP/1.1 server returning a near-empty HTML page.
/// Near-empty DOM -> low paint/DOM scores -> confidence below 0.60 -> escalation.
async fn start_fixture_server() -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let body = b"<!DOCTYPE html><html><head></head><body></body></html>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Type: text/html\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\
                     \r\n",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.write_all(body).await;
            });
        }
    });

    Ok((addr, handle))
}

fn is_loopback_endpoint(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("://127.0.0.1")
        || lower.contains("://localhost")
        || lower.contains("://[::1]")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires SERVO_WEBDRIVER_URL and SERVO_SECONDARY_WEBDRIVER_URL to be set"]
async fn escalation_handoff_produces_migrated_response() -> Result<()> {
    // Skip cleanly when endpoints are not configured.
    if std::env::var("SERVO_WEBDRIVER_URL").is_err()
        || std::env::var("SERVO_SECONDARY_WEBDRIVER_URL").is_err()
    {
        eprintln!(
            "[week12] skipping: SERVO_WEBDRIVER_URL or \
             SERVO_SECONDARY_WEBDRIVER_URL not set"
        );
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("PNEUMA_LOG").unwrap_or_else(|_| {
                "pneuma=debug,pneuma_broker=debug,pneuma_engines=debug".into()
            }),
        )
        .try_init()
        .ok();

    let primary_endpoint = std::env::var("SERVO_WEBDRIVER_URL").unwrap_or_default();

    // Remote endpoints (e.g. CI via tunnel) cannot reach runner-local 127.0.0.1,
    // so use a deterministic data: URL fixture in that case.
    let (fixture_url, fixture_task) = if is_loopback_endpoint(&primary_endpoint) {
        let (addr, task) = start_fixture_server().await?;
        let url = format!("http://{addr}/");
        eprintln!("[week12] local fixture server at {url}");
        (url, Some(task))
    } else {
        eprintln!("[week12] remote endpoint detected; using data URL fixture");
        (DATA_FIXTURE_URL.to_string(), None)
    };

    // Build primary engine, broker, and JS runtime.
    let engine = Box::new(pneuma_engines::servo::ServoEngine::launch().await?);
    let (broker_tx, broker_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = pneuma_broker::handle::BrokerHandle::new(broker_tx);
    tokio::spawn(pneuma_broker::service::run(broker_rx, engine));
    let runtime = pneuma_js::Runtime::new(handle)?;

    // Synchronous FFI script - no ghost.open, no JSON.stringify on the result object.
    // nav1: primary navigate triggers low-confidence escalation; successful
    // handoff reply is stamped migrated:true by broker.
    // nav2: subsequent navigate is served on secondary and also stamped.
    let script = format!(
        r#"
        var pageId = __pneuma_private_ffi.createPage();
        var nav1 = JSON.parse(__pneuma_private_ffi.navigate(pageId, "{url}", "{{}}"));
        var nav2 = JSON.parse(__pneuma_private_ffi.navigate(pageId, "{url}", "{{}}"));
        globalThis.__pneuma_week12_result = {{
            nav1_ok:       nav1.ok       === true,
            nav1_engine:   nav1.engine   ?? "unknown",
            nav1_migrated: nav1.migrated === true,
            nav2_ok:       nav2.ok       === true,
            nav2_engine:   nav2.engine   ?? "unknown",
            nav2_migrated: nav2.migrated === true
        }};
        "#,
        url = fixture_url
    );

    runtime.execute_script(&script)?;

    // Retrieve via direct eval - no JSON.stringify to avoid double-encoding.
    assert_eq!(
        runtime.eval_expression("__pneuma_week12_result.nav1_ok")?,
        "true",
        "nav1 should succeed"
    );
    assert_eq!(
        runtime.eval_expression("__pneuma_week12_result.nav1_engine === 'servo'")?,
        "true",
        "nav1 should be served by primary Servo"
    );
    assert_eq!(
        runtime.eval_expression("__pneuma_week12_result.nav1_migrated")?,
        "true",
        "nav1 should be migrated because handoff completed during first navigate"
    );
    assert_eq!(
        runtime.eval_expression("__pneuma_week12_result.nav2_ok")?,
        "true",
        "nav2 should succeed"
    );
    assert_eq!(
        runtime.eval_expression("__pneuma_week12_result.nav2_engine === 'servo'")?,
        "true",
        "nav2 should be served by Servo (primary or secondary proxy)"
    );
    assert_eq!(
        runtime.eval_expression("__pneuma_week12_result.nav2_migrated")?,
        "true",
        "nav2 should be stamped migrated after escalation handoff"
    );

    eprintln!("[week12] all assertions passed");
    if let Some(task) = fixture_task {
        task.abort();
    }
    Ok(())
}
