//! Servo -> Ladybird escalation smoke test.
//!
//! Requires:
//! - `--features ladybird`
//! - `LADYBIRD_BUILD_DIR` set to a working Ladybird build directory
//! - `SERVO_WEBDRIVER_URL` set to a reachable Servo endpoint
//!
//! Run with:
//!   CC=clang-20 CXX=clang++-20 \
//!   LADYBIRD_BUILD_DIR=/path/to/ladybird/Build/debug-clang20 \
//!   SERVO_WEBDRIVER_URL=http://localhost:4444 \
//!   cargo test -p pneuma-core --test escalation_to_ladybird_smoke \
//!     --features ladybird -- --ignored --nocapture

#[cfg(feature = "ladybird")]
use anyhow::Result;
#[cfg(feature = "ladybird")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(feature = "ladybird")]
use tokio::net::TcpListener;
#[cfg(feature = "ladybird")]
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(feature = "ladybird")]
use std::time::Duration;
#[cfg(feature = "ladybird")]
use std::fs;

#[cfg(feature = "ladybird")]
fn is_loopback_endpoint(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("://127.0.0.1")
        || lower.contains("://localhost")
        || lower.contains("://[::1]")
}

#[cfg(feature = "ladybird")]
fn is_wsl_host_endpoint(url: &str) -> bool {
    let host = match url
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .and_then(|hp| hp.rsplit_once(':').map(|(h, _)| h).or(Some(hp)))
    {
        Some(h) if !h.is_empty() => h,
        _ => return false,
    };

    let osrelease = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    if !osrelease.to_ascii_lowercase().contains("microsoft") {
        return false;
    }

    let resolv = fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    let nameserver = resolv
        .lines()
        .find_map(|line| line.strip_prefix("nameserver "))
        .map(str::trim)
        .unwrap_or("");
    !nameserver.is_empty() && host == nameserver
}

#[cfg(feature = "ladybird")]
fn endpoint_is_reachable(url: &str) -> bool {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let host_port = without_scheme.split('/').next().unwrap_or("");
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().unwrap_or(0)),
        None => (host_port, 80),
    };

    if host.is_empty() || port == 0 {
        return false;
    }

    let addr = match (host, port).to_socket_addrs().ok().and_then(|mut it| it.next()) {
        Some(addr) => addr,
        None => return false,
    };

    TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_ok()
}

#[cfg(feature = "ladybird")]
async fn start_fixture_server() -> Result<(std::net::SocketAddr, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };

            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");

                let body = if path == "/healthy" {
                    // DOM + text-rich page to avoid immediate low-confidence escalation.
                    "<!DOCTYPE html><html><head><title>EscalationSeed</title></head><body>\
                     <main><h1>Seed</h1><p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                     Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.</p>\
                     <ul><li>one</li><li>two</li><li>three</li><li>four</li><li>five</li></ul>\
                     <section><article><p>alpha</p><p>beta</p><p>gamma</p></article></section>\
                     </main></body></html>"
                } else {
                    // Intentionally sparse page to push confidence below threshold.
                    "<!DOCTYPE html><html><head><title>EscalationTest</title></head><body></body></html>"
                };

                let response = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Type: text/html\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\
                     \r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });

    Ok((addr, handle))
}

#[cfg(feature = "ladybird")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ladybird feature, LADYBIRD_BUILD_DIR, Servo endpoint, service binaries"]
async fn low_confidence_servo_result_escalates_to_ladybird() -> Result<()> {
    if std::env::var("LADYBIRD_BUILD_DIR").is_err() {
        eprintln!("[escalation-ladybird] skipping: LADYBIRD_BUILD_DIR not set");
        return Ok(());
    }
    let primary_endpoint = std::env::var("SERVO_WEBDRIVER_URL").unwrap_or_default();
    if primary_endpoint.is_empty() {
        eprintln!("[escalation-ladybird] skipping: SERVO_WEBDRIVER_URL not set");
        return Ok(());
    }
    if !is_loopback_endpoint(&primary_endpoint) && !is_wsl_host_endpoint(&primary_endpoint) {
        eprintln!(
            "[escalation-ladybird] skipping: SERVO_WEBDRIVER_URL must be loopback (or WSL host IP) for local fixture"
        );
        return Ok(());
    }
    if !endpoint_is_reachable(&primary_endpoint) {
        eprintln!("[escalation-ladybird] skipping: SERVO_WEBDRIVER_URL not reachable");
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("PNEUMA_LOG")
                .unwrap_or_else(|_| "pneuma=debug,pneuma_broker=debug,pneuma_engines=debug".into()),
        )
        .try_init()
        .ok();

    let (addr, fixture_task) = start_fixture_server().await?;
    let healthy_url = format!("http://{addr}/healthy");
    let low_url = format!("http://{addr}/low");

    let engine = Box::new(pneuma_engines::servo::ServoEngine::launch().await?);
    let (broker_tx, broker_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = pneuma_broker::handle::BrokerHandle::new(broker_tx);
    tokio::spawn(pneuma_broker::service::run(broker_rx, engine));

    let handle_for_nav = handle.clone();
    let (_r1, r2) = tokio::task::spawn_blocking(move || -> Result<(String, String)> {
        let page_id = handle_for_nav.create_page()?;

        // Seed state on Servo primary so handoff hits import_state on Ladybird.
        let _ = handle_for_nav.navigate(page_id, healthy_url, "{}".to_string())?;
        let _ = handle_for_nav.evaluate(
            page_id,
            "document.cookie='ladder_cookie=1; path=/'; localStorage.setItem('ladder_ls','1'); 'ok';"
                .to_string(),
        )?;

        let first = handle_for_nav.navigate(page_id, low_url.clone(), "{}".to_string())?;
        let second = handle_for_nav.navigate(page_id, low_url, "{}".to_string())?;
        Ok((first, second))
    })
    .await??;

    assert!(
        r2.contains(r#""migrated":true"#),
        "expected second navigate to be migrated, got: {r2}"
    );
    assert!(
        r2.contains(r#""engine":"ladybird""#),
        "expected second navigate to use ladybird engine, got: {r2}"
    );
    assert!(
        r2.contains("EscalationTest"),
        "expected title to contain EscalationTest, got: {r2}"
    );

    let handle_for_shutdown = handle.clone();
    let _ = tokio::task::spawn_blocking(move || handle_for_shutdown.shutdown()).await;
    fixture_task.abort();
    eprintln!("[escalation-ladybird] escalation to ladybird smoke passed");
    Ok(())
}
