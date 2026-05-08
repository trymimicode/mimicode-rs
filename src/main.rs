use anyhow::Result;

mod agent;
mod providers;
mod types;

use types::{Message, MessageContent};

#[tokio::main]
async fn main() -> Result<()> {
    let messages = vec![
        Message {
            role: "user".into(),
            content: MessageContent::Text("Say 'proof of life' and nothing else.".into()),
        },
    ];

    let reply = providers::call_claude(&messages, "", "claude-haiku-4-5-20251001").await?;

    match reply.content {
        types::MessageContent::Blocks(blocks) => {
            for block in blocks {
                if let types::ContentBlock::Text { text } = block {
                    println!("{}", text);
                }
            }
        }
        types::MessageContent::Text(t) => println!("{}", t),
    }

    Ok(())
}
