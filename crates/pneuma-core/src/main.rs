use anyhow::Result;
use clap::Parser;

mod cli;
use cli::Args;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("PNEUMA_LOG").unwrap_or_else(|_| "pneuma=info".into()))
        .init();

    let args = Args::parse();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Pneuma starting");

    match args.command {
        cli::Command::Run {
            script,
            engine,
            stealth,
            ..
        } => run_script(script, engine, stealth).await,
        cli::Command::Eval { expression, engine } => eval_expression(expression, engine).await,
        cli::Command::Serve { port, .. } => serve(port).await,
    }
}

fn parse_initial_transport_profile() -> Result<Option<pneuma_engines::TransportStealthProfile>> {
    let Ok(raw) = std::env::var("PNEUMA_INITIAL_TRANSPORT_PROFILE") else {
        return Ok(None);
    };
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(None);
    }

    let profile = match value.as_str() {
        "chrome120" | "chrome_120" | "chrome-120" | "chrome" => {
            pneuma_engines::TransportStealthProfile::Chrome120
        }
        "safari17" | "safari_17" | "safari-17" | "safari" => {
            pneuma_engines::TransportStealthProfile::Safari17
        }
        "firefox123" | "firefox_123" | "firefox-123" | "firefox" => {
            pneuma_engines::TransportStealthProfile::Firefox123
        }
        _ => anyhow::bail!(
            "unsupported PNEUMA_INITIAL_TRANSPORT_PROFILE value: {raw}. supported values: chrome120, safari17, firefox123"
        ),
    };
    Ok(Some(profile))
}

async fn spawn_broker_handle(
    engine: cli::EngineChoice,
    stealth: bool,
) -> Result<pneuma_broker::handle::BrokerHandle> {
    let template = pneuma_broker::LaunchTemplate {
        kind: engine.into(),
        stealth,
        initial_transport: parse_initial_transport_profile()?,
    };
    let (broker_tx, broker_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = pneuma_broker::handle::BrokerHandle::new(broker_tx);
    tokio::spawn(pneuma_broker::service::run_lazy(broker_rx, template));
    Ok(handle)
}

async fn run_script(script: std::path::PathBuf, engine: cli::EngineChoice, stealth: bool) -> Result<()> {
    let source = std::fs::read_to_string(&script)?;

    let handle = spawn_broker_handle(engine, stealth).await?;
    let runtime = pneuma_js::Runtime::new(handle)?;
    runtime.execute_script(&source)?;

    // TODO(week-9): replace direct CLI engine selection with confidence-based routing.
    tracing::info!(
        backend = runtime.backend_name(),
        path = ?script,
        ?engine,
        stealth,
        "executed script"
    );

    Ok(())
}

async fn eval_expression(expr: String, engine: cli::EngineChoice) -> Result<()> {
    tracing::info!("evaluating expression");
    let handle = spawn_broker_handle(engine, false).await?;
    let runtime = pneuma_js::Runtime::new(handle)?;
    let rendered = runtime.eval_expression(&expr)?;
    println!("{rendered}");
    Ok(())
}

async fn serve(port: u16) -> Result<()> {
    tracing::info!(port, "starting server mode");
    println!("serve on :{}", port);
    Ok(())
}
