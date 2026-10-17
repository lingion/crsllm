//! Google provider adapter.
//!
//! Translates between the unified Anthropic format and Google's
//! Generative Language API (content/generateContent).

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{BaseAdapter, ProviderAdapter, ProviderResult};
use crate::config::ProviderConfig;
use crate::error::{classify_error, ErrorClass, ProviderError};
use crate::translator::{
    AnthropicContent, AnthropicContentBlock, AnthropicMessage, AnthropicRequest,
    AnthropicResponse, AnthropicResponseContent,
};

pub struct GoogleAdapter {
    base: BaseAdapter,
}

impl GoogleAdapter {
    pub fn new(config: ProviderConfig, model_id: String) -> Self {
        Self {
            base: BaseAdapter::new("google", config, model_id),
        }
    }
}

#[async_trait]
impl ProviderAdapter for GoogleAdapter {
    fn name(&self) -> &'static str {
        "google"
    }

    /// Translate Anthropic-format request → Google generateContent JSON.
    fn translate_request(&self, req: &AnthropicRequest, _model_id: &str) -> Value {
        let mut contents: Vec<Value> = Vec::new();
        let mut system_instruction: Option<String> = None;

        if let Some(sys) = &req.system {
            system_instruction = Some(sys.clone());
        }

        for msg in &req.messages {
            let entry = convert_message(&msg, &mut contents);
            if let Some(sys) = entry {
                system_instruction = Some(sys);
            }
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
            "contents": contents,
        });

        if let Some(sys) = system_instruction {
            body["system_instruction"] = json!({
                "parts": [{"text": sys}]
            });
        }

        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }

        if !tools.is_empty() {
            body["tool_config"] = json!({
                "function_calling_mode": "ANY"
            });
        }

        body
    }

    fn translate_response(&self, resp: &Value) -> serde_json::Result<AnthropicResponse> {
        let empty: Vec<Value> = Vec::new();
        let candidates = resp.get("candidates").and_then(|v| v.as_array()).unwrap_or(&empty);

        let mut content = Vec::new();
        let mut stop_reason = "stop".to_string();
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;

        if let Some(candidate) = candidates.first() {
            if let Some(parts) = candidate.get("content").and_then(|v| v.as_array()) {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        content.push(AnthropicResponseContent::Text {
                            text: text.to_string(),
                        });
                    }
                    if let Some(fc) = part.get("function_call") {
                        let name = fc.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let args_str = fc.get("args").and_then(|v| v.as_str()).unwrap_or("{}");
                        let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                        content.push(AnthropicResponseContent::ToolUse {
                            id: uuid::Uuid::new_v4().to_string(),
                            name,
                            input,
                        });
                    }
                }
            }

            if let Some(reason) = candidate.get("finishReason").and_then(|v| v.as_str()) {
                stop_reason = match reason.to_uppercase().as_str() {
                    "STOP" => "stop",
                    "MAX_TOKENS" => "max_tokens",
                    "SAFETY" => "refusal",
                    other => other,
                }
                .to_string();
            }
        }

        if let Some(usage) = resp.get("usageMetadata") {
            input_tokens = usage.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            output_tokens = usage.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        }

        Ok(AnthropicResponse {
            id: uuid::Uuid::new_v4().to_string(),
            response_type: "message".to_string(),
            role: "assistant".to_string(),
            model: "unknown".to_string(),
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
            .post(self.base.url(&format!("/v1/models/{}:generateContent", self.base.model_id)))
            .header("x-goog-api-key", self.base.auth_header())
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
        // Google streaming — translate request and add alt=sse
        let body = self.translate_request(req, model_id);
        self.base
            .client
            .post(self.base.url(&format!(
                "/v1/models/{}:streamGenerateContent?alt=sse",
                self.base.model_id
            )))
            .header("x-goog-api-key", self.base.auth_header())
            .json(&body)
            .send()
            .await
    }

    async fn list_models(&self) -> Result<Vec<String>, reqwest::Error> {
        let resp = self
            .base
            .client
            .get(self.base.url(&format!("models?key={}", self.base.auth_header())))
            .send()
            .await?;

        let parsed: Value = resp.json().await?;

        let mut models = Vec::new();
        if let Some(model_list) = parsed.get("models").and_then(|v| v.as_array()) {
            for item in model_list {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    models.push(name.to_string());
                }
            }
        }
        Ok(models)
    }
}

fn convert_message(msg: &AnthropicMessage, contents: &mut Vec<Value>) -> Option<String> {
    if msg.role == "system" {
        if let Some(content) = &msg.content {
            if let AnthropicContent::Text(s) = content {
                return Some(s.clone());
            }
        }
        return None;
    }

    let mut parts: Vec<Value> = Vec::new();

    if let Some(content) = &msg.content {
        match content {
            AnthropicContent::Text(s) => {
                parts.push(json!({ "text": s }));
            }
            AnthropicContent::Blocks(blocks) => {
                for block in blocks {
                    match block {
                        AnthropicContentBlock::Text { text, .. } => {
                            parts.push(json!({ "text": text }));
                        }
                        AnthropicContentBlock::ToolUse { .. } => {
                            // Tool use handled in role dispatch below
                        }
                        AnthropicContentBlock::ToolResult { content, .. } => {
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
                            parts.push(json!({
                                "text": result_text
                            }));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    let role = if msg.role == "assistant" { "model" } else { "user" };
    contents.push(json!({ "role": role, "parts": parts }));
    None
}
