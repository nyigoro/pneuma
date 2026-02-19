use anyhow::Result;
use clap::Parser;
use pneuma_engines::servo::ServoEngine;

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

async fn spawn_broker_handle(engine: cli::EngineChoice) -> Result<pneuma_broker::handle::BrokerHandle> {
    let runtime_engine: Box<dyn pneuma_engines::HeadlessEngine> = match engine {
        cli::EngineChoice::Servo => Box::new(ServoEngine::launch().await?),
        cli::EngineChoice::Ladybird => anyhow::bail!("ladybird engine is not wired yet"),
    };

    let (broker_tx, broker_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = pneuma_broker::handle::BrokerHandle::new(broker_tx);
    tokio::spawn(pneuma_broker::service::run(broker_rx, runtime_engine));
    Ok(handle)
}

async fn run_script(script: std::path::PathBuf, engine: cli::EngineChoice, stealth: bool) -> Result<()> {
    let source = std::fs::read_to_string(&script)?;

    let handle = spawn_broker_handle(engine).await?;
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
    let handle = spawn_broker_handle(engine).await?;
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
