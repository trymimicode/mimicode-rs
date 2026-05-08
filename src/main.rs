use anyhow::Result;
use clap::Parser;

mod agent;
mod providers;
mod types;

#[derive(Parser)]
#[command(name = "mimicode", about = "A minimal CLI coding agent")]
struct Cli {
    /// Prompt to send; omit to enter interactive mode
    prompt: Option<String>,

    /// Anthropic API key
    #[arg(long, env = "ANTHROPIC_API_KEY")]
    api_key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    agent::run(&cli.api_key, cli.prompt).await
}
