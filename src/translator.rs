//! Unified internal format = Anthropic format.
//!
//! All providers translate to/from this internal representation.
//!
//! This module defines the unified request/response types (Anthropic-style)
//! and translation dispatch between them and provider-specific formats.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Unified internal request — Anthropic Messages API format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<AnthropicTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String, // "user" | "assistant" | "system"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<AnthropicContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

impl From<String> for AnthropicContent {
    fn from(s: String) -> Self {
        AnthropicContent::Text(s)
    }
}

impl From<Vec<AnthropicContentBlock>> for AnthropicContent {
    fn from(b: Vec<AnthropicContentBlock>) -> Self {
        AnthropicContent::Blocks(b)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicContentBlock {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        citations: Option<Value>,
    },
    Image {
        source: AnthropicImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: ToolResultContent,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicImageSource {
    #[serde(rename = "type")]
    pub media_type: String, // "base64"
    #[serde(rename = "media_type")]
    pub format: String, // "image/png", "image/jpeg", etc.
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value, // JSON schema
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caching: Option<bool>,
}

/// Unified internal response — Anthropic Messages API format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub response_type: String, // "message"
    pub role: String,
    pub model: String,
    pub content: Vec<AnthropicResponseContent>,
    pub stop_reason: String,
    pub stop_sequence: Option<String>,
    pub usage: AnthropicUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicResponseContent {
    Text { text: String },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Convert an Anthropic-format request to a provider-specific request.
/// Note: actual translation is done by the adapter's `translate_request` method.
pub fn to_provider_request(
    req: &AnthropicRequest,
    provider: &str,
) -> serde_json::Result<serde_json::Value> {
    match provider {
        "anthropic" => to_anthropic_request(req),
        _ => Ok(serde_json::to_value(req).unwrap_or(json!({}))),
    }
}

/// Convert a provider-specific response to the unified Anthropic response.
/// Note: actual translation is done by the adapter's `translate_response` method.
pub fn from_provider_response(
    resp: &serde_json::Value,
    _provider: &str,
) -> serde_json::Result<AnthropicResponse> {
    serde_json::from_value(resp.clone())
}

fn to_anthropic_request(req: &AnthropicRequest) -> serde_json::Result<serde_json::Value> {
    serde_json::to_value(req)
}

fn from_anthropic_response(resp: &serde_json::Value) -> serde_json::Result<AnthropicResponse> {
    serde_json::from_value(resp.clone())
}

/// Build an Anthropic error response.
pub fn anthropic_error(message: &str) -> Value {
    serde_json::json!({
        "type": "error",
        "error": {
            "type": "api_error",
            "message": message,
        }
    })
}

/// Extract the error message from a provider response body.
pub fn extract_error_message(body: &str, provider: &str) -> String {
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(body);
    if let Ok(val) = parsed {
        match provider {
            "openai" => val
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| body.to_string()),
            "google" => val
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| body.to_string()),
            "anthropic" => val
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| body.to_string()),
            _ => body.to_string(),
        }
    } else {
        body.to_string()
    }
}
