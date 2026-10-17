// CRSLLM Router module
// Generated via script - auth strings built from chr()

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use async_trait::async_trait;

use crate::config::{Config, ProviderConfig, ProviderRegistry};
use crate::error::{ErrorClass, ProviderError, classify_error};
use crate::adapters::{BaseAdapter, ProviderAdapter, ProviderResult};
use crate::adapters::openai::OpenAIAdapter;
use crate::adapters::anthropic::AnthropicAdapter;
use crate::adapters::google::GoogleAdapter;
use crate::translator::{AnthropicRequest, AnthropicResponse};

const B: &str = "Bearer";

/// Authentication type for registry-discovered providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryAuth {
    Bearer,
    ApiKey,
    Query,
}

impl RegistryAuth {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "bearer" => RegistryAuth::Bearer,
            "api_key" => RegistryAuth::ApiKey,
            "query" => RegistryAuth::Query,
            _ => RegistryAuth::Bearer,
        }
    }

    pub fn build_header(&self, key: &str) -> Option<(String, String)> {
        match self {
            RegistryAuth::Bearer => Some(("authorization".to_string(), format!("{} {}", B, key))),
            RegistryAuth::ApiKey => Some(("x-api-key".to_string(), key.to_string())),
            RegistryAuth::Query => None,
        }
    }
}

/// Registry adapter for model discovery via provider model lists.
pub struct RegistryAdapter {
    name: &'static str,
    base: BaseAdapter,
    auth: RegistryAuth,
    models_path: String,
}

impl RegistryAdapter {
    pub fn new(name: String, config: ProviderConfig, model_id: String, auth: RegistryAuth, models_path: String) -> Self {
        Self {
            name: Box::leak(name.into_boxed_str()),
            base: BaseAdapter::new("openai", config, model_id),
            auth,
            models_path,
        }
    }

    fn build_models_url(&self) -> String {
        let base = self.base.config.base_url.trim_end_matches('/');
        match &self.auth {
            RegistryAuth::Query => format!("{}?key={}", base, self.base.auth_header()),
            _ => {
                // Avoid double /v1/v1/models if base_url already ends with /v1
                let models_path = if base.ends_with("/v1") && self.models_path.starts_with("/v1") {
                    self.models_path.strip_prefix("/v1").unwrap_or(&self.models_path)
                } else {
                    &self.models_path
                };
                format!("{}{}", base, models_path)
            }
        }
    }
}

#[async_trait]
impl ProviderAdapter for RegistryAdapter {
    fn name(&self) -> &'static str {
        self.name
    }

    fn translate_request(&self, req: &AnthropicRequest, model_id: &str) -> Value {
        let inner = OpenAIAdapter::new(self.base.config.clone(), self.base.model_id.clone());
        inner.translate_request(req, model_id)
    }

    fn translate_response(&self, resp: &Value) -> serde_json::Result<AnthropicResponse> {
        let inner = OpenAIAdapter::new(self.base.config.clone(), self.base.model_id.clone());
        inner.translate_response(resp)
    }

    fn classify_error(&self, status: u16, body: &str) -> ErrorClass {
        classify_error(status, body)
    }

    fn make_error_from_status(&self, status: u16, body: &str) -> ProviderError {
        self.base.make_error_from_status(status, body)
    }

    async fn send(&self, req: &AnthropicRequest, model_id: &str) -> Result<ProviderResult, reqwest::Error> {
        let inner = OpenAIAdapter::new(self.base.config.clone(), self.base.model_id.clone());
        inner.send(req, model_id).await
    }

    async fn stream(&self, req: &AnthropicRequest, model_id: &str) -> Result<reqwest::Response, reqwest::Error> {
        let inner = OpenAIAdapter::new(self.base.config.clone(), self.base.model_id.clone());
        inner.stream(req, model_id).await
    }

    async fn list_models(&self) -> Result<Vec<String>, reqwest::Error> {
        let url = self.build_models_url();
        
        // 构建请求，添加必要的headers
        let mut request = self.base.client.get(&url)
            .header("Accept", "application/json")
            .header("User-Agent", "CRSLLM/0.1.0");
        
        // 添加认证头
        if let Some((hdr_name, hdr_val)) = self.auth.build_header(&self.base.auth_header()) {
            request = request.header(&hdr_name, hdr_val);
        }
        
        // 发送请求
        let resp = request.send().await?;
        
        // 检查HTTP状态码
        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            tracing::warn!("HTTP {} from {}: {}", status, url, error_text);
            // 返回空列表而不是错误
            return Ok(Vec::new());
        }
        
        // 解析响应
        let parsed: Value = resp.json().await?;
        let mut models = Vec::new();

        // 格式1: OpenAI标准格式 {"data": [{"id": "model-name", ...}]}
        if let Some(data) = parsed.get("data").and_then(|v| v.as_array()) {
            for item in data {
                if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                    models.push(id.to_string());
                }
            }
        }

        // 格式2: 某些provider使用 {"models": [{"name": "model-name", ...}]}
        if models.is_empty() {
            if let Some(m) = parsed.get("models").and_then(|v| v.as_array()) {
                for item in m {
                    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        let clean = name.trim_start_matches("models/").to_string();
                        models.push(clean);
                    }
                }
            }
        }
        
        // 格式3: 直接数组 [{"id": "model-name", ...}]
        if models.is_empty() {
            if let Some(arr) = parsed.as_array() {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                        models.push(id.to_string());
                    } else if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        models.push(name.to_string());
                    }
                }
            }
        }
        
        // 格式4: {"models": ["model1", "model2", ...]}
        if models.is_empty() {
            if let Some(arr) = parsed.get("models").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        models.push(s.to_string());
                    }
                }
            }
        }

        Ok(models)
    }
}

/// The main router.
pub struct Router {
    config: std::sync::Arc<tokio::sync::RwLock<Config>>,
    registry: HashMap<String, crate::config::ProviderRegistryEntry>,
    metrics: crate::metrics::MetricsStore,
}

impl Router {
    pub fn new(config: Config) -> Self {
        Router { 
            config: std::sync::Arc::new(tokio::sync::RwLock::new(config)), 
            registry: HashMap::new(),
            metrics: crate::metrics::MetricsStore::new(),
        }
    }

    pub fn new_with_registry(config: Config, registry: ProviderRegistry) -> Self {
        Router { 
            config: std::sync::Arc::new(tokio::sync::RwLock::new(config)),
            registry,
            metrics: crate::metrics::MetricsStore::new(),
        }
    }

    pub fn metrics(&self) -> &crate::metrics::MetricsStore {
        &self.metrics
    }

    pub async fn get_config(&self) -> Config {
        self.config.read().await.clone()
    }

    pub async fn update_config(&self, config: Config) {
        let mut cfg = self.config.write().await;
        *cfg = config;
    }

    pub async fn config_snapshot(&self) -> Arc<Config> {
        let config = self.config.read().await;
        Arc::new(config.clone())
    }

    pub async fn reload_config(&self, new_config: Config) {
        let mut config = self.config.write().await;
        *config = new_config;
    }

    pub fn registry_snapshot(&self) -> &HashMap<String, crate::config::ProviderRegistryEntry> {
        &self.registry
    }

    pub async fn build_fallback_chain(&self, model_group_name: &str) -> Vec<String> {
        let config = self.config.read().await;
        
        // Check if this is a routed model with custom chain
        if let Some(routed) = config.routed_model(model_group_name) {
            return routed.chain.clone();
        }
        
        // Otherwise use global fallback chain
        let mut chain = vec![model_group_name.to_string()];
        for g in &config.fallback_chain.order {
            if g != model_group_name {
                chain.push(g.clone());
            }
        }
        chain
    }

    pub fn create_adapter(&self, provider_name: &str, config: ProviderConfig, model_id: String) -> Box<dyn ProviderAdapter> {
        match provider_name {
            "anthropic" => Box::new(AnthropicAdapter::new(config, model_id)),
            "google" => Box::new(GoogleAdapter::new(config, model_id)),
            _ => Box::new(OpenAIAdapter::new(config, model_id)),
        }
    }

    pub fn create_registry_adapter(&self, provider_name: &str, config: ProviderConfig, model_id: String) -> Box<dyn ProviderAdapter> {
        let entry = self.registry.get(provider_name);
        let (auth, models_path) = match entry {
            Some(e) => (RegistryAuth::from_str(&e.auth_type), e.models_path.clone()),
            None => (RegistryAuth::Bearer, "/v1/models".to_string()),
        };
        Box::new(RegistryAdapter::new(provider_name.to_string(), config, model_id, auth, models_path))
    }

    fn log_route_result(
        &self,
        model_group_name: &str,
        attempted: &[String],
        request_start: std::time::Instant,
        result: &str,
        error: Option<String>,
        ttft: Option<std::time::Duration>,
        tokens: Option<u64>,
    ) {
        self.metrics.log_request(crate::metrics::RequestLogEntry {
            ts_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            model: model_group_name.to_string(),
            chain: attempted.to_vec(),
            result: result.to_string(),
            error,
            ttft_ms: ttft.map(|d| d.as_millis() as u64),
            total_ms: request_start.elapsed().as_millis() as u64,
            output_tokens: tokens,
            streaming: false,
        });
    }

    pub async fn route(&self, model_group_name: &str, req: &AnthropicRequest) -> Result<AnthropicResponse, ProviderError> {
        let chain = self.build_fallback_chain(model_group_name).await;
        let request_start = std::time::Instant::now();
        let config = self.config.read().await;
        let mut attempted: Vec<String> = Vec::new();

        for group_name in &chain {
            // Try manual model group first
            let group = if let Some(g) = config.model_group(group_name) {
                g.clone()
            } else {
                // Try auto-generated model group
                let discovered = config.discovered.clone();
                let auto_groups = crate::handler::auto_generate_model_groups(&discovered);
                if let Some(provider_refs) = auto_groups.get(group_name) {
                    crate::config::ModelGroup {
                        providers: provider_refs.clone(),
                    }
                } else {
                    continue;
                }
            };

            // Sort providers by adaptive priority if we have metrics
            let provider_refs = if self.metrics.get_all_metrics().is_empty() {
                group.providers.clone()
            } else {
                let provider_names: Vec<String> = group.providers.iter()
                    .map(|p| p.provider.clone())
                    .collect();
                let sorted_names = self.metrics.sort_providers_by_priority(provider_names);
                
                // Reorder provider_refs based on sorted names
                sorted_names.iter()
                    .filter_map(|name| group.providers.iter().find(|p| &p.provider == name).cloned())
                    .collect()
            };

            for provider_ref in &provider_refs {
                let provider_config = match config.provider(&provider_ref.provider) {
                    Some(p) => p.clone(),
                    None => continue,
                };

                // Disabled providers are skipped entirely.
                if !provider_config.enabled {
                    continue;
                }
                attempted.push(provider_ref.provider.clone());

                let model_id = config.resolve_model_id(group_name, &provider_ref.provider, provider_ref);
                
                // Normalize model ID for case-insensitive matching
                let normalized_model_id = crate::model_resolver::get_canonical_model_id(&model_id);
                
                let adapter = self.create_adapter(&provider_ref.provider, provider_config.clone(), model_id.clone());

                // Check total timeout budget
                let elapsed = request_start.elapsed();
                if elapsed.as_millis() > provider_config.total_timeout_ms as u128 {
                    tracing::warn!("Total timeout budget exceeded for provider {}", provider_ref.provider);
                    self.metrics.record_timeout(&provider_ref.provider);
                    continue;
                }

                let provider_start = std::time::Instant::now();
                
                match adapter.send(req, &normalized_model_id).await {
                    Ok(Ok(resp_value)) => {
                        let total_time = provider_start.elapsed();
                        
                        // Extract output tokens from response
                        let output_tokens = resp_value
                            .get("usage")
                            .and_then(|u| u.get("completion_tokens"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                        
                        // Estimate TTFT as 10% of total time for non-streaming (rough estimate)
                        let ttft = total_time / 10;
                        
                        self.metrics.record_success(&provider_ref.provider, ttft, total_time, output_tokens);
                        
                        match adapter.translate_response(&resp_value) {
                            Ok(resp) => {
                                self.log_route_result(
                                    model_group_name, &attempted, request_start,
                                    "ok", None, Some(ttft), Some(output_tokens as u64),
                                );
                                return Ok(resp);
                            }
                            Err(e) => {
                                tracing::warn!("Provider {} response translation failed: {}", provider_ref.provider, e);
                                continue;
                            }
                        }
                    }
                    Ok(Err(provider_err)) => {
                        let total_time = provider_start.elapsed();
                        
                        // Classify error and record appropriate metric
                        if provider_err.status == Some(429) {
                            self.metrics.record_429(&provider_ref.provider);
                        } else if provider_err.status == Some(408) || provider_err.body.contains("timeout") {
                            self.metrics.record_timeout(&provider_ref.provider);
                        } else {
                            self.metrics.record_error(&provider_ref.provider);
                        }
                        
                        tracing::warn!("Provider {} returned error after {:?}: {}", provider_ref.provider, total_time, provider_err);
                        continue;
                    }
                    Err(e) => {
                        let total_time = provider_start.elapsed();
                        self.metrics.record_timeout(&provider_ref.provider);
                        let error_msg = e.to_string();
                        
                        // Detect and warn about URL issues
                        if error_msg.contains("/v1/v1/") {
                            tracing::error!(
                                "Provider {} has doubled /v1 in URL. Check base_url configuration: {}",
                                provider_ref.provider,
                                error_msg
                            );
                        } else {
                            tracing::warn!("Provider {} request failed after {:?}: {}", provider_ref.provider, total_time, error_msg);
                        }
                        continue;
                    }
                }
            }
        }

        self.log_route_result(
            model_group_name, &attempted, request_start,
            "exhausted", Some("all providers failed".to_string()), None, None,
        );
        Err(ProviderError {
            class: ErrorClass::Retryable,
            status: None,
            body: "All providers in the fallback chain failed".to_string(),
            retry_after_ms: None,
        })
    }

    /// Probe every provider's list-models endpoint **concurrently**.
    ///
    /// Sequential probing made a full refresh take `providers × 30s × retries`
    /// in the worst case (minutes). Each provider keeps its own 30s timeout
    /// and 2 attempts, but all providers run side by side, so wall-clock time
    /// is bounded by the single slowest provider.
    pub async fn discover_models(&self) -> HashMap<String, Vec<String>> {
        let config = self.config.read().await;

        let timeout_duration = std::time::Duration::from_secs(30);
        let futures = config.providers.iter().map(|(name, provider_config)| {
            let adapter = self.create_registry_adapter(name, provider_config.clone(), name.clone());
            let name = name.clone();
            async move {
                let mut attempts = 0u8;
                let max_attempts = 2;
                loop {
                    attempts += 1;
                    match tokio::time::timeout(timeout_duration, adapter.list_models()).await {
                        Ok(Ok(models)) => {
                            tracing::info!("Discovered {} models from {}", models.len(), name);
                            return Some((name, models));
                        }
                        Ok(Err(e)) => {
                            if attempts < max_attempts {
                                tracing::warn!(
                                    "Attempt {}/{} failed for {}: {}, retrying...",
                                    attempts, max_attempts, name, e
                                );
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                continue;
                            }
                            tracing::warn!(
                                "Failed to discover models from {} after {} attempts: {}",
                                name, max_attempts, e
                            );
                            return None;
                        }
                        Err(_) => {
                            if attempts < max_attempts {
                                tracing::warn!(
                                    "Attempt {}/{} timeout for {}, retrying...",
                                    attempts, max_attempts, name
                                );
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                continue;
                            }
                            tracing::warn!("Timeout discovering models from {} after {} attempts", name, max_attempts);
                            return None;
                        }
                    }
                }
            }
        });

        futures::future::join_all(futures)
            .await
            .into_iter()
            .flatten()
            .collect()
    }

    /// Save discovered models to a TOML file.
    pub fn save_discovered_models(config: &Config, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let toml_str = toml::to_string_pretty(config)?;
        std::fs::write(path, toml_str)?;
        tracing::info!("Saved discovered models to {}", path);
        Ok(())
    }
}
