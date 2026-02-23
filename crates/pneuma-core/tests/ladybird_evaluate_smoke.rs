//! Ladybird evaluate smoke test.
//!
//! Requires:
//! - `--features ladybird`
//! - `LADYBIRD_BUILD_DIR` set to a working Ladybird build directory
//!
//! Run with:
//!   CC=clang-20 CXX=clang++-20 \
//!   LADYBIRD_BUILD_DIR=/path/to/ladybird/Build/debug-clang20 \
//!   cargo test -p pneuma-core --test ladybird_evaluate_smoke \
//!     --features ladybird -- --ignored --nocapture

#[cfg(feature = "ladybird")]
use anyhow::Result;

#[cfg(feature = "ladybird")]
use pneuma_engines::{ladybird::LadybirdEngine, HeadlessEngine};

#[cfg(feature = "ladybird")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ladybird feature, LADYBIRD_BUILD_DIR, and service binaries"]
async fn ladybird_evaluate_returns_expression_result() -> Result<()> {
    if std::env::var("LADYBIRD_BUILD_DIR").is_err() {
        eprintln!("[ladybird-eval] skipping: LADYBIRD_BUILD_DIR not set");
        return Ok(());
    }

    let engine = LadybirdEngine::launch()?;

    let _navigate_meta = engine
        .navigate("data:text/html,<title>LadybirdEvalSmoke</title>", "{}")
        .await?;

    let result = engine.evaluate("1 + 2").await?;
    assert_eq!(result, "3", "1 + 2 should evaluate to 3, got: {result:?}");

    let title = engine.evaluate("'LadybirdEvalSmoke'").await?;
    assert!(
        title.contains("LadybirdEvalSmoke"),
        "string literal should contain 'LadybirdEvalSmoke', got: {title:?}"
    );

    eprintln!("[ladybird-eval] all assertions passed");
    Ok(())
}
