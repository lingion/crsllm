//! Anthropic provider adapter.
//!
//! Internal format = Anthropic format, so this adapter is largely passthrough.
//! The main work: HTTP request construction, SSE streaming, error parsing.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{BaseAdapter, ProviderAdapter, ProviderResult};
use crate::config::ProviderConfig;
use crate::error::{classify_error, ErrorClass, ProviderError};
use crate::translator::{AnthropicRequest, AnthropicResponse};

pub struct AnthropicAdapter {
    base: BaseAdapter,
}

impl AnthropicAdapter {
    pub fn new(config: ProviderConfig, model_id: String) -> Self {
        Self {
            base: BaseAdapter::new("anthropic", config, model_id),
        }
    }
}

#[async_trait]
impl ProviderAdapter for AnthropicAdapter {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn translate_request(&self, req: &AnthropicRequest, model_id: &str) -> Value {
        // Anthropic request is our internal format — just override the model.
        let mut val = serde_json::to_value(req).unwrap_or(json!({}));
        if let Value::Object(ref mut obj) = val {
            obj.insert("model".to_string(), json!(model_id));
        }
        val
    }

    fn translate_response(&self, resp: &Value) -> serde_json::Result<AnthropicResponse> {
        serde_json::from_value(resp.clone())
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
            .post(self.base.url("/v1/messages"))
            .header("x-api-key", self.base.auth_header())
            .header("anthropic-version", "2023-06-01")
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
        _model_id: &str,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let body = self.translate_request(req, &self.base.model_id);
        self.base
            .client
            .post(self.base.url("/v1/messages"))
            .header("x-api-key", self.base.auth_header())
            .header("anthropic-version", "2023-06-01")
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
    }

    async fn list_models(&self) -> Result<Vec<String>, reqwest::Error> {
        let resp = self
            .base
            .client
            .get(self.base.url("/v1/models"))
            .header("x-api-key", self.base.auth_header())
            .header("anthropic-version", "2023-06-01")
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
