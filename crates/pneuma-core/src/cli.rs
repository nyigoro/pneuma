use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "pneuma", version, about = "Headless browser orchestration runtime")]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Run {
        script: PathBuf,
        #[arg(long, value_enum, default_value_t = EngineChoice::Servo)]
        engine: EngineChoice,
        #[arg(long, default_value_t = false)]
        stealth: bool,
        #[arg(long)]
        profile: Option<PathBuf>,
    },
    Eval {
        expression: String,
        #[arg(long, value_enum, default_value_t = EngineChoice::Servo)]
        engine: EngineChoice,
    },
    Serve {
        #[arg(long, default_value_t = 3000)]
        port: u16,
        #[arg(long, value_enum, default_value_t = EngineChoice::Servo)]
        engine: EngineChoice,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum EngineChoice {
    Servo,
    Ladybird,
}

impl From<EngineChoice> for pneuma_engines::EngineKind {
    fn from(value: EngineChoice) -> Self {
        match value {
            EngineChoice::Servo => pneuma_engines::EngineKind::Servo,
            EngineChoice::Ladybird => pneuma_engines::EngineKind::Ladybird,
        }
    }
}
