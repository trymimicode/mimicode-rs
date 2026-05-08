use std::io::{self, BufRead, Write};

use anyhow::Result;

use crate::providers::call_claude;
use crate::types::{ContentBlock, Message, MessageContent};

const MODEL: &str = "claude-haiku-4-5-20251001";

pub async fn agent_turn(
    user_msg: &str,
    history: &mut Vec<Message>,
    system: &str,
) -> Result<String> {
    history.push(Message { role: "user".into(), content: MessageContent::Text(user_msg.into()) });

    let reply = call_claude(history, system, MODEL).await?;
    let text = extract_text(&reply);
    history.push(reply);

    Ok(text)
}

pub async fn run(system: &str, prompt: Option<String>) -> Result<()> {
    let mut history: Vec<Message> = Vec::new();

    if let Some(text) = prompt {
        let reply = agent_turn(&text, &mut history, system).await?;
        println!("{}", reply);
        return Ok(());
    }

    loop {
        let input = read_line()?;
        let trimmed = input.trim();

        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "/exit" || trimmed == "/quit" {
            break;
        }

        let reply = agent_turn(trimmed, &mut history, system).await?;
        println!("\n{}\n", reply);
    }

    Ok(())
}

fn extract_text(msg: &Message) -> String {
    match &msg.content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn read_line() -> Result<String> {
    print!("> ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim_end_matches('\n').trim_end_matches('\r').to_string())
}
