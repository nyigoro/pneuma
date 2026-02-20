//! Week 15 screenshot smoke test.
//!
//! Requires one live Servo WebDriver endpoint: SERVO_WEBDRIVER_URL
//!
//! Run with:
//!   cargo test -p pneuma-core --test week15_screenshot_smoke -- --ignored --nocapture

use anyhow::Result;

#[tokio::test]
#[ignore = "requires SERVO_WEBDRIVER_URL to be set"]
async fn screenshot_pipeline_returns_valid_png_base64() -> Result<()> {
    if std::env::var("SERVO_WEBDRIVER_URL").is_err() {
        eprintln!("[week15] skipping: SERVO_WEBDRIVER_URL not set");
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("PNEUMA_LOG").unwrap_or_else(|_| "pneuma=debug,pneuma_engines=debug".into()),
        )
        .try_init()
        .ok();

    let engine = Box::new(pneuma_engines::servo::ServoEngine::launch().await?);
    let (broker_tx, broker_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = pneuma_broker::handle::BrokerHandle::new(broker_tx);
    tokio::spawn(pneuma_broker::service::run(broker_rx, engine));
    let runtime = pneuma_js::Runtime::new(handle)?;

    // Data URL fixture - no external network required.
    let fixture_url = "data:text/html,<html><head></head><body><h1>Week15</h1></body></html>";

    let script = format!(
        r#"
        var pageId = __pneuma_private_ffi.createPage();
        var nav = JSON.parse(__pneuma_private_ffi.navigate(pageId, "{url}", "{{}}"));
        __pneuma_private_ffi.setViewport(pageId, 1280, 720);
        var vp = __pneuma_private_ffi.getViewport(pageId);
        var b64 = __pneuma_private_ffi.screenshot(pageId);
        globalThis.__week15 = {{
            nav_ok:           nav.ok === true,
            viewport_w:       vp[0],
            viewport_h:       vp[1],
            // +-2px tolerance: Linux window managers may adjust dimensions to
            // tile boundaries or compositor constraints.
            viewport_ok:      Math.abs(vp[0] - 1280) <= 2 && Math.abs(vp[1] - 720) <= 2,
            screenshot_type:  typeof b64,
            screenshot_len:   b64.length,
            is_nontrivial:    b64.length > 100,
            has_png_header:   b64.startsWith("iVBORw0KGgo"),
            no_data_prefix:   !b64.startsWith("data:"),
        }};
        "#,
        url = fixture_url
    );

    runtime.execute_script(&script)?;

    assert_eq!(
        runtime.eval_expression("__week15.nav_ok")?,
        "true",
        "navigate should succeed"
    );
    assert_eq!(
        runtime.eval_expression("__week15.viewport_ok")?,
        "true",
        "viewport should be within +-2px of 1280x720 before screenshot"
    );
    assert_eq!(
        runtime.eval_expression("__week15.screenshot_type")?,
        "\"string\"",
        "screenshot should be a string"
    );
    assert_eq!(
        runtime.eval_expression("__week15.is_nontrivial")?,
        "true",
        "screenshot should have non-trivial length"
    );
    assert_eq!(
        runtime.eval_expression("__week15.has_png_header")?,
        "true",
        "screenshot should start with PNG base64 magic header"
    );
    assert_eq!(
        runtime.eval_expression("__week15.no_data_prefix")?,
        "true",
        "screenshot should be plain base64, not a data URL"
    );

    eprintln!(
        "[week15] screenshot length: {}",
        runtime.eval_expression("__week15.screenshot_len")?
    );
    eprintln!("[week15] all assertions passed");
    Ok(())
}
