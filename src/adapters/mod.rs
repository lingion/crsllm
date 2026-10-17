#![allow(dead_code)]
pub mod anthropic;
pub mod google;
pub mod openai;

use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

use crate::config::ProviderConfig;
use crate::error::{classify_error, ErrorClass, ProviderError};
use crate::translator::AnthropicRequest;

/// Result of a provider call: either a JSON value (response body) or an error.
pub type ProviderResult = Result<Value, ProviderError>;

/// A provider adapter handles translation, HTTP calls, and error classification
/// for a specific provider (OpenAI, Anthropic, Google).
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Provider name (e.g. "openai", "anthropic", "google").
    fn name(&self) -> &'static str;

    /// Translate an internal Anthropic-format request to this provider's JSON.
    fn translate_request(&self, req: &AnthropicRequest, model_id: &str) -> Value;

    /// Translate this provider's JSON response to Anthropic format.
    fn translate_response(&self, resp: &Value) -> serde_json::Result<crate::translator::AnthropicResponse>;

    /// Classify an error from this provider.
    fn classify_error(&self, status: u16, body: &str) -> ErrorClass;

    /// Build a ProviderError from a status + body (no retry-after header).
    fn make_error_from_status(&self, status: u16, body: &str) -> ProviderError;

    /// Send a non-streaming request.
    async fn send(&self, req: &AnthropicRequest, model_id: &str) -> Result<ProviderResult, reqwest::Error>;

    /// Send a streaming request; returns the response body as a stream of
    /// provider-specific SSE bytes. Caller is responsible for translating.
    async fn stream(
        &self,
        req: &AnthropicRequest,
        model_id: &str,
    ) -> Result<reqwest::Response, reqwest::Error>;

    /// Fetch the list of available model IDs from this provider.
    /// Returns a list of native model names (e.g. "gpt-4o", "claude-3-5-sonnet-20241022").
    async fn list_models(&self) -> Result<Vec<String>, reqwest::Error>;
}

/// Base struct with shared HTTP client + config.
pub struct BaseAdapter {
    pub provider_name: &'static str,
    pub client: reqwest::Client,
    pub config: ProviderConfig,
    pub model_id: String,
}

impl BaseAdapter {
    pub fn new(provider_name: &'static str, config: ProviderConfig, model_id: String) -> Self {
        let timeout = Duration::from_millis(config.read_timeout_ms);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_millis(config.connect_timeout_ms))
            .build()
            .expect("Failed to build reqwest client");
        Self {
            provider_name,
            client,
            config,
            model_id,
        }
    }

    pub fn url(&self, path: &str) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        
        // Avoid double /v1: if base ends with /v1 and path starts with v1/, strip it from path
        let normalized_path = if base.ends_with("/v1") && path.starts_with("v1/") {
            path.strip_prefix("v1/").unwrap_or(path)
        } else {
            path
        };
        
        format!("{}/{}", base, normalized_path)
    }

    pub fn auth_header(&self) -> String {
        self.config.api_key.clone()
    }

    /// Build a ProviderError from a status + body.
    pub fn make_error(&self, status: u16, body: &str, retry_after_header: Option<&str>) -> ProviderError {
        let class = classify_error(status, body);
        let retry_after_ms = crate::error::parse_retry_after_ms(status, body, retry_after_header);
        ProviderError {
            class,
            status: Some(status),
            body: body.to_string(),
            retry_after_ms,
        }
    }

    /// Shortcut for making an error without retry_after header.
    pub fn make_error_from_status(&self, status: u16, body: &str) -> ProviderError {
        self.make_error(status, body, None)
    }

    /// Classify an error (default: delegates to crate-level classify_error).
    /// Adapters can use this or provide their own.
    pub fn classify_error(&self, status: u16, body: &str) -> ErrorClass {
        classify_error(status, body)
    }
}
