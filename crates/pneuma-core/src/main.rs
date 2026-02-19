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
        cli::Command::Eval { expression, .. } => eval_expression(expression).await,
        cli::Command::Serve { port, .. } => serve(port).await,
    }
}

fn spawn_broker_handle() -> pneuma_broker::handle::BrokerHandle {
    let (broker_tx, broker_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = pneuma_broker::handle::BrokerHandle::new(broker_tx);
    tokio::spawn(pneuma_broker::service::run(broker_rx));
    handle
}

async fn run_script(script: std::path::PathBuf, engine: cli::EngineChoice, stealth: bool) -> Result<()> {
    let source = std::fs::read_to_string(&script)?;

    let handle = spawn_broker_handle();
    let runtime = pneuma_js::Runtime::new(handle)?;
    runtime.execute_script(&source)?;

    // TODO(week-7): `engine` here reflects the CLI selection, not active broker engine routing.
    tracing::info!(
        backend = runtime.backend_name(),
        path = ?script,
        ?engine,
        stealth,
        "executed script"
    );

    Ok(())
}

async fn eval_expression(expr: String) -> Result<()> {
    tracing::info!("evaluating expression");
    let handle = spawn_broker_handle();
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
