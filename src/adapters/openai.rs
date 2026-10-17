//! OpenAI provider adapter.
//!
//! Translates between the unified Anthropic format and OpenAI's
//! chat completions format.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{BaseAdapter, ProviderAdapter, ProviderResult};
use crate::config::ProviderConfig;
use crate::error::{classify_error, ErrorClass, ProviderError};
use crate::translator::{
    AnthropicContent, AnthropicContentBlock, AnthropicMessage, AnthropicRequest,
    AnthropicResponse, AnthropicResponseContent,
};

pub struct OpenAIAdapter {
    base: BaseAdapter,
}

impl OpenAIAdapter {
    pub fn new(config: ProviderConfig, model_id: String) -> Self {
        Self {
            base: BaseAdapter::new("openai", config, model_id),
        }
    }
}

#[async_trait]
impl ProviderAdapter for OpenAIAdapter {
    fn name(&self) -> &'static str {
        "openai"
    }

    /// Translate Anthropic-format request → OpenAI chat completions JSON.
    fn translate_request(&self, req: &AnthropicRequest, model_id: &str) -> Value {
        let mut messages: Vec<Value> = Vec::new();

        // System message
        if let Some(system) = &req.system {
            messages.push(json!({
                "role": "system",
                "content": system
            }));
        }

        // Convert each message
        for msg in &req.messages {
            let entry = convert_anthropic_message_to_openai(msg);
            messages.push(entry);
        }

        let mut tools = Vec::new();
        for t in &req.tools {
            tools.push(json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description.as_deref().unwrap_or(""),
                    "parameters": t.input_schema
                }
            }));
        }

        let mut body = json!({
            "model": model_id,
            "messages": messages,
            "stream": false,
        });

        if let Some(max) = req.max_tokens {
            body["max_tokens"] = json!(max);
        }
        if let Some(temp) = req.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(top_p) = req.top_p {
            body["top_p"] = json!(top_p);
        }
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }

        body
    }

    fn translate_response(&self, resp: &Value) -> serde_json::Result<AnthropicResponse> {
        let id = resp
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("chatcmpl-unknown")
            .to_string();
        let model = resp.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let empty: Vec<Value> = Vec::new();
        let choices = resp.get("choices").and_then(|v| v.as_array()).unwrap_or(&empty);

        let mut content = Vec::new();
        let mut stop_reason = "stop".to_string();
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;

        if let Some(choice) = choices.first() {
            if let Some(msg) = choice.get("message") {
                if let Some(text) = msg.get("content").and_then(|v| v.as_str()) {
                    content.push(AnthropicResponseContent::Text {
                        text: text.to_string(),
                    });
                }
                if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tool_calls {
                        if let Some(func) = tc.get("function") {
                            let name = func.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let args_str = func.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
                            let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                            content.push(AnthropicResponseContent::ToolUse {
                                id: tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                name,
                                input,
                            });
                        }
                    }
                }
                if let Some(r) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                    stop_reason = match r {
                        "stop" => "stop",
                        "length" => "max_tokens",
                        "tool_calls" => "tool_use",
                        other => other,
                    }
                    .to_string();
                }
            }
        }

        if let Some(usage) = resp.get("usage") {
            input_tokens = usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            output_tokens = usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        }

        Ok(AnthropicResponse {
            id,
            response_type: "message".to_string(),
            role: "assistant".to_string(),
            model,
            content,
            stop_reason,
            stop_sequence: None,
            usage: crate::translator::AnthropicUsage {
                input_tokens,
                output_tokens,
            },
        })
    }

    fn classify_error(&self, status: u16, body: &str) -> ErrorClass {
        classify_error(status, body)
    }

    fn make_error_from_status(&self, status: u16, body: &str) -> ProviderError {
        self.base.make_error_from_status(status, body)
    }

    async fn send(&self, req: &AnthropicRequest, model_id: &str) -> Result<ProviderResult, reqwest::Error> {
        let body = self.translate_request(req, model_id);
        let resp = self
            .base
            .client
            .post(self.base.url("/v1/chat/completions"))
            .header("authorization", format!("Bearer {}", self.base.auth_header()))
            .json(&body)
            .send()
            .await?;

        let headers = resp.headers().clone();
        let status = resp.status().as_u16();
        let text = resp.text().await?;
        if status >= 400 {
            let retry_after = headers.get("retry-after").and_then(|v| v.to_str().ok());
            Ok(Err(self.base.make_error(status, &text, retry_after)))
        } else {
            let parsed: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => return Ok(Err(ProviderError {
                    class: ErrorClass::ProviderBadRequest,
                    status: Some(status),
                    body: format!("JSON parse error: {}", e),
                    retry_after_ms: None,
                })),
            };
            Ok(Ok(parsed))
        }
    }

    async fn stream(
        &self,
        req: &AnthropicRequest,
        model_id: &str,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let mut body = self.translate_request(req, model_id);
        body["stream"] = json!(true);

        self.base
            .client
            .post(self.base.url("/v1/chat/completions"))
            .header("authorization", format!("Bearer {}", self.base.auth_header()))
            .json(&body)
            .send()
            .await
    }

    async fn list_models(&self) -> Result<Vec<String>, reqwest::Error> {
        let resp = self
            .base
            .client
            .get(self.base.url("/v1/models"))
            .header("authorization", format!("Bearer {}", self.base.auth_header()))
            .send()
            .await?;

        let parsed: Value = resp.json().await?;

        let mut models = Vec::new();
        if let Some(data) = parsed.get("data").and_then(|v| v.as_array()) {
            for item in data {
                if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                    models.push(id.to_string());
                }
            }
        }
        Ok(models)
    }
}

/// Convert an Anthropic message to an OpenAI message.
fn convert_anthropic_message_to_openai(msg: &AnthropicMessage) -> Value {
    let role = msg.role.clone();

    // Handle content — text, blocks, or tool_use/tool_result
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<Value> = Vec::new();

    if let Some(content) = &msg.content {
        match content {
            AnthropicContent::Text(s) => {
                if !s.is_empty() {
                    text_parts.push(s.clone());
                }
            }
            AnthropicContent::Blocks(blocks) => {
                for block in blocks {
                    match block {
                        AnthropicContentBlock::Text { text, .. } => {
                            text_parts.push(text.clone());
                        }
                        AnthropicContentBlock::ToolUse { id, name, input } => {
                            let args_str = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
                            tool_calls.push(json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": args_str
                                }
                            }));
                        }
                        AnthropicContentBlock::ToolResult { tool_use_id, content, .. } => {
                            let result_text = match content {
                                crate::translator::ToolResultContent::Text(t) => t.clone(),
                                crate::translator::ToolResultContent::Blocks(blocks) => {
                                    blocks.iter().filter_map(|b| {
                                        if let AnthropicContentBlock::Text { text, .. } = b {
                                            Some(text.clone())
                                        } else {
                                            None
                                        }
                                    }).collect::<Vec<_>>().join("")
                                }
                            };
                            text_parts.push(json!({
                                "tool_call_id": tool_use_id,
                                "content": result_text
                            }).to_string());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if !tool_calls.is_empty() {
        let content = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        };
        let mut val = json!({ "role": role, "tool_calls": tool_calls });
        if let Some(c) = content {
            val["content"] = json!(c);
        }
        val
    } else if !text_parts.is_empty() {
        json!({ "role": role, "content": text_parts.join("\n") })
    } else if !tool_calls.is_empty() {
        json!({ "role": role, "tool_calls": tool_calls })
    } else {
        json!({ "role": role, "content": None::<String> })
    }
}
