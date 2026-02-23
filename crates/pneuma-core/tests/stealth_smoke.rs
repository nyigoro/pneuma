//! Stealth smoke test.
//!
//! Requires one live Servo WebDriver endpoint: SERVO_WEBDRIVER_URL
//!
//! Run with:
//!   cargo test -p pneuma-core --test stealth_smoke -- --ignored --nocapture

use anyhow::Result;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires SERVO_WEBDRIVER_URL to be set"]
async fn stealth_patches_navigator_webdriver() -> Result<()> {
    if std::env::var("SERVO_WEBDRIVER_URL").is_err() {
        eprintln!("[stealth-smoke] skipping: SERVO_WEBDRIVER_URL not set");
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

    runtime.execute_script(
        r#"
        (async () => {
            var page = await ghost.open(
                "data:text/html,<title>Stealth</title>",
                { stealth_level: 1 }
            );
            globalThis.__stealth_result = {
                webdriver: await page.evaluate(() => navigator.webdriver),
                platform: await page.evaluate(() => navigator.platform),
            };
        })();
    "#,
    )?;

    let webdriver = runtime.eval_expression("__stealth_result.webdriver")?;
    assert_eq!(webdriver, "false", "navigator.webdriver should be false");

    let platform = runtime.eval_expression("__stealth_result.platform")?;
    assert_eq!(platform, "\"Win32\"", "navigator.platform should be Win32");

    runtime.execute_script(
        r#"
        (async () => {
            var page = await ghost.open(
                "data:text/html,<title>Stealth2</title>",
                { stealth_level: 2 }
            );
            globalThis.__stealth2 = {
                vendor: await page.evaluate(() => navigator.vendor),
                deviceMemory: await page.evaluate(() => navigator.deviceMemory),
                screenWidth: await page.evaluate(() => screen.width),
            };
        })();
    "#,
    )?;

    let vendor = runtime.eval_expression("__stealth2.vendor")?;
    assert_eq!(vendor, "\"Google Inc.\"", "navigator.vendor should be Google Inc.");

    let mem = runtime.eval_expression("__stealth2.deviceMemory")?;
    assert_eq!(mem, "8", "navigator.deviceMemory should be 8");

    let sw = runtime.eval_expression("__stealth2.screenWidth")?;
    assert_eq!(sw, "1920", "screen.width should be 1920");

    runtime.execute_script(
        r#"
        (async () => {
            var page = await ghost.open(
                "data:text/html,<title>Stealth3</title>",
                { stealth_level: 3 }
            );
            globalThis.__stealth3 = {
                canvasNonEmpty: await page.evaluate(() => {
                    const c = document.createElement("canvas");
                    c.width = 10;
                    c.height = 10;
                    const ctx = c.getContext("2d");
                    ctx.fillStyle = "red";
                    ctx.fillRect(0, 0, 10, 10);
                    return c.toDataURL().length > 0;
                }),
                arialPresent: await page.evaluate(() =>
                    document.fonts ? document.fonts.check("16px Arial") : null
                ),
                fakeAbsent: await page.evaluate(() =>
                    document.fonts ? !document.fonts.check("16px FakeFontXYZ123") : null
                ),
            };
        })();
    "#,
    )?;

    let canvas = runtime.eval_expression("__stealth3.canvasNonEmpty")?;
    assert_eq!(canvas, "true", "canvas toDataURL should be non-empty");

    let arial = runtime.eval_expression("__stealth3.arialPresent")?;
    assert_eq!(arial, "true", "Arial should be present");

    let fake = runtime.eval_expression("__stealth3.fakeAbsent")?;
    assert_eq!(fake, "true", "FakeFontXYZ123 should be absent");

    Ok(())
}
