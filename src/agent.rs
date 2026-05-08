use std::io::{self, BufRead, Write};

use anyhow::Result;

use crate::providers::call_claude;
use crate::types::{ContentBlock, Message, MessageContent};

const MODEL: &str = "claude-haiku-4-5-20251001";

pub async fn run(system: &str, prompt: Option<String>) -> Result<()> {
    let mut messages: Vec<Message> = Vec::new();

    if let Some(text) = prompt {
        messages.push(Message { role: "user".into(), content: MessageContent::Text(text) });
        let reply = call_claude(&messages, system, MODEL).await?;
        println!("{}", extract_text(&reply));
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

        messages.push(Message { role: "user".into(), content: MessageContent::Text(trimmed.into()) });
        let reply = call_claude(&messages, system, MODEL).await?;
        println!("\n{}\n", extract_text(&reply));
        messages.push(reply);
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
