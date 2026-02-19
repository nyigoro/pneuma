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

async fn run_script(script: std::path::PathBuf, engine: cli::EngineChoice, stealth: bool) -> Result<()> {
    let source = std::fs::read_to_string(&script)?;

    let _broker = pneuma_broker::Broker::new(engine.into(), stealth)?;
    let _runtime = pneuma_js::Runtime::new()?;

    tracing::info!(path = ?script, "executing script");
    println!("{}", source);

    Ok(())
}

async fn eval_expression(expr: String) -> Result<()> {
    tracing::info!("evaluating expression");
    println!("eval: {}", expr);
    Ok(())
}

async fn serve(port: u16) -> Result<()> {
    tracing::info!(port, "starting server mode");
    println!("serve on :{}", port);
    Ok(())
}
