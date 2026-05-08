use anyhow::Result;
use clap::Parser;

mod agent;
mod providers;
mod types;

const SYSTEM: &str = "You are a minimal CLI coding agent. Be concise and precise.";

#[derive(Parser)]
#[command(name = "mimicode", about = "A minimal CLI coding agent")]
struct Cli {
    /// Optional session name (reserved for future persistence)
    #[arg(short, long)]
    session: Option<String>,

    /// Prompt to send; omit to enter interactive mode
    prompt: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    agent::run(SYSTEM, cli.prompt).await
}
