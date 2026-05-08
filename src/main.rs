use anyhow::Result;

mod agent;
mod providers;
mod types;

use types::Message;

#[tokio::main]
async fn main() -> Result<()> {
    let mut history: Vec<Message> = Vec::new();
    let system = "";

    let r1 = agent::agent_turn("My name is Paul. Just say 'got it'.", &mut history, system).await?;
    println!("Turn 1: {}", r1);

    let r2 = agent::agent_turn("What is my name?", &mut history, system).await?;
    println!("Turn 2: {}", r2);

    Ok(())
}
