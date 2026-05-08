use std::io::{self, BufRead, Write};

use anyhow::Result;
use reqwest::Client;

use crate::providers::call_claude;
use crate::types::{ContentBlock, Message, MessageContent};

pub async fn run(api_key: &str, prompt: Option<String>) -> Result<()> {
    let client = Client::new();
    let mut messages: Vec<Message> = Vec::new();

    if let Some(text) = prompt {
        messages.push(Message { role: "user".into(), content: MessageContent::Text(text) });
        let resp = call_claude(&client, api_key, &messages).await?;
        println!("{}", extract_text(&resp.content));
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
        let resp = call_claude(&client, api_key, &messages).await?;
        let text = extract_text(&resp.content);
        println!("\n{}\n", text);
        messages.push(Message { role: "assistant".into(), content: MessageContent::Text(text) });
    }

    Ok(())
}

fn extract_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn read_line() -> Result<String> {
    print!("> ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim_end_matches('\n').trim_end_matches('\r').to_string())
}
