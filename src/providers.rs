use anyhow::{Context, Result};
use reqwest::Client;

use crate::types::{ApiRequest, ApiResponse, Message};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-haiku-4-5-20251001";
const MAX_TOKENS: u32 = 8096;

pub async fn call_claude(client: &Client, api_key: &str, messages: &[Message]) -> Result<ApiResponse> {
    let request = ApiRequest {
        model: MODEL.to_string(),
        max_tokens: MAX_TOKENS,
        messages: messages.to_vec(),
    };

    let response = client
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .context("Failed to send request to Anthropic API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("API error {}: {}", status, body);
    }

    response
        .json::<ApiResponse>()
        .await
        .context("Failed to parse API response")
}
