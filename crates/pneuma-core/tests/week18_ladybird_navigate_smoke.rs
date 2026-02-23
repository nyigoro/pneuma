//! Week 18 Ladybird broker smoke test.
//!
//! Requires:
//! - `--features ladybird`
//! - `LADYBIRD_BUILD_DIR` set to a working Ladybird build directory
//!
//! Run with:
//!   CC=clang-20 CXX=clang++-20 \
//!   LADYBIRD_BUILD_DIR=/path/to/ladybird/Build/debug-clang20 \
//!   cargo test -p pneuma-core --test week18_ladybird_navigate_smoke \
//!     --features ladybird -- --ignored --nocapture

#[cfg(feature = "ladybird")]
use anyhow::Result;

#[cfg(feature = "ladybird")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ladybird feature, LADYBIRD_BUILD_DIR, and service binaries"]
async fn ladybird_navigate_via_broker_returns_title() -> Result<()> {
    if std::env::var("LADYBIRD_BUILD_DIR").is_err() {
        eprintln!("[week18] skipping: LADYBIRD_BUILD_DIR not set");
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("PNEUMA_LOG")
                .unwrap_or_else(|_| "pneuma=debug,pneuma_broker=debug,pneuma_engines=debug".into()),
        )
        .try_init()
        .ok();

    let engine = Box::new(pneuma_engines::ladybird::LadybirdEngine::launch()?);
    let (broker_tx, broker_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = pneuma_broker::handle::BrokerHandle::new(broker_tx);
    tokio::spawn(pneuma_broker::service::run(broker_rx, engine));
    let handle_for_nav = handle.clone();
    let response = tokio::task::spawn_blocking(move || -> Result<String> {
        let page_id = handle_for_nav.create_page()?;
        handle_for_nav.navigate(
            page_id,
            "data:text/html,<title>Week18</title><h1>ok</h1>".to_string(),
            "{}".to_string(),
        )
    })
    .await??;

    assert!(
        response.contains(r#""ok":true"#),
        "expected ok=true in response, got: {response}"
    );
    assert!(
        response.contains(r#""engine":"ladybird""#),
        "expected engine=ladybird, got: {response}"
    );
    assert!(
        response.contains("Week18"),
        "expected title to contain Week18, got: {response}"
    );

    let handle_for_shutdown = handle.clone();
    let _ = tokio::task::spawn_blocking(move || handle_for_shutdown.shutdown()).await;

    eprintln!("[week18] broker Ladybird navigate smoke passed");
    Ok(())
}
