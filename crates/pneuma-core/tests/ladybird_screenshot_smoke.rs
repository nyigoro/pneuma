#[tokio::test]
#[ignore = "requires ladybird feature, LADYBIRD_BUILD_DIR, and service binaries"]
async fn ladybird_screenshot_and_viewport_smoke() -> anyhow::Result<()> {
    if std::env::var("LADYBIRD_BUILD_DIR").is_err() {
        return Ok(());
    }

    use pneuma_engines::ladybird::engine::LadybirdEngine;
    use pneuma_engines::traits::HeadlessEngine;

    let engine = LadybirdEngine::launch()?;

    engine.set_viewport(800, 600).await?;
    let (w, h) = engine.get_viewport().await?;
    assert_eq!((w, h), (800, 600), "viewport should be 800x600 after set");

    engine
        .navigate("data:text/html,<title>Shot</title><h1>ok</h1>", "{}")
        .await?;

    let bytes = engine.screenshot().await?;
    assert!(!bytes.is_empty(), "screenshot bytes should not be empty");

    eprintln!("[ladybird-screenshot] captured {} bytes", bytes.len());
    Ok(())
}
