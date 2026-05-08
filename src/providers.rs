use std::env;

use anyhow::{Context, Result};
use reqwest::Client;

use crate::types::{ApiRequest, ApiResponse, Message, MessageContent};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MAX_TOKENS: u32 = 8096;

pub async fn call_claude(messages: &[Message], system: &str, model: &str) -> Result<Message> {
    let api_key = env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;

    let request = ApiRequest {
        model: model.to_string(),
        max_tokens: MAX_TOKENS,
        system: if system.is_empty() { None } else { Some(system.to_string()) },
        messages: messages.to_vec(),
    };

    let client = Client::new();
    let response = client
        .post(API_URL)
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .context("failed to reach Anthropic API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("API error {}: {}", status, body);
    }

    let api_response = response
        .json::<ApiResponse>()
        .await
        .context("failed to parse API response")?;

    Ok(Message {
        role: api_response.role,
        content: MessageContent::Blocks(api_response.content),
    })
}
