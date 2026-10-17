//! HTTP handlers for the CRSLLM gateway.
//!
//! - `/v1/messages` — Anthropic Messages API (primary internal format)
//! - `/v1/chat/completions` — OpenAI format (translated to Anthropic internally)
//! - `/health` — health check

use axum::{
    extract::{Request, State, Json, Path},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::router::Router;
use crate::translator;
use crate::streaming;

/// Shared state holding the router.
pub type AppState = Arc<Router>;

/// Health check endpoint.
pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// True when the client-supplied credential matches a configured gateway key.
/// Accepts Anthropic-style `x-api-key` and OpenAI-style `Authorization: Bearer …`.
pub fn client_key_ok(headers: &HeaderMap, keys: &[String]) -> bool {
    let candidate = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|a| a.strip_prefix("Bearer ").or(a.strip_prefix("bearer ")))
                .map(|s| s.trim().to_string())
        });
    match candidate {
        Some(k) if !k.is_empty() => keys.iter().any(|configured| configured == &k),
        _ => false,
    }
}

/// Gateway auth middleware.
///
/// - `/v1/*` (model traffic: /v1/messages, /v1/chat/completions, /v1/models) requires a
///   valid key whenever `[gateway] api_keys` is non-empty. Empty allow-list = open access
///   (local dev default), so the gateway never locks itself out silently.
/// - `/api/*` (console control plane: keys, providers, groups, logs) and `/dashboard` are
///   loopback-only: the console is meant to run on the gateway machine, not be shouted
///   over the LAN (it binds 0.0.0.0 for serving models).
pub async fn gateway_auth(State(router): State<AppState>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let is_model = path.starts_with("/v1");
    let is_control = path.starts_with("/api") || path == "/dashboard";
    if !is_model && !is_control {
        return next.run(req).await; // /health, /metrics stay open
    }

    if is_control {
        let loopback = req
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0.ip().is_loopback())
            .unwrap_or(true); // no ConnectInfo (e.g. tests) → treat as local
        if !loopback {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "gateway control plane is loopback-only"})),
            )
                .into_response();
        }
    }

    let config = router.get_config().await;
    if is_model && !config.gateway.api_keys.is_empty()
        && !client_key_ok(req.headers(), &config.gateway.api_keys)
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "type": "error",
                "error": {"type": "authentication_error", "message": "invalid or missing API key"}
            })),
        )
            .into_response();
    }

    next.run(req).await
}

/// Get all API keys (masked; full keys are shown once at creation time only).
pub async fn get_api_keys(State(router): State<AppState>) -> Response {
    let config = router.get_config().await;
    let keys = &config.gateway.api_keys;
    let masked_keys: Vec<_> = keys
        .iter()
        .map(|k| json!({ "key": crate::config::mask_key(k) }))
        .collect();

    (StatusCode::OK, Json(json!({ "api_keys": masked_keys }))).into_response()
}

/// Create a new API key
pub async fn create_api_key(
    State(router): State<AppState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Response {
    let new_key = if req.key.is_empty() {
        // Generate a random key
        format!("crsllm-{}", uuid::Uuid::new_v4())
    } else {
        req.key
    };
    
    // Save to config
    let mut config = router.get_config().await;
    config.gateway.api_keys.push(new_key.clone());
    
    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response();
    }
    router.reload_config(config).await;

    (StatusCode::CREATED, Json(json!({
        "key": crate::config::mask_key(&new_key),
        "full_key": new_key,
    }))).into_response()
}

/// Delete an API key. Accepts the full key OR its masked form (as shown in the
/// console list), so a key created in an earlier browser session is still deletable.
pub async fn delete_api_key(
    State(router): State<AppState>,
    Path(key): Path<String>,
) -> Response {
    let mut config = router.get_config().await;
    let before = config.gateway.api_keys.len();
    config.gateway.api_keys.retain(|k| {
        k != &key && crate::config::mask_key(k) != key
    });
    if config.gateway.api_keys.len() == before {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "key not found"})),
        )
            .into_response();
    }

    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response();
    }
    router.reload_config(config).await;

    (StatusCode::OK, Json(json!({"status": "deleted"}))).into_response()
}

/// Metrics endpoint - returns provider performance metrics.
pub async fn get_metrics(State(router): State<AppState>) -> Response {
    let all_metrics = router.metrics().get_all_metrics();
    let mut json_map = serde_json::Map::new();
    
    for (provider_name, metrics) in all_metrics {
        let provider_json = json!({
            "total_requests": metrics.total_requests,
            "success_count": metrics.success_count,
            "timeout_count": metrics.timeout_count,
            "error_429_count": metrics.error_429_count,
            "success_rate": metrics.success_rate(),
            "timeout_rate": metrics.timeout_rate(),
            "error_429_rate": metrics.error_429_rate(),
            "median_ttft_ms": metrics.median_ttft().map(|d: std::time::Duration| d.as_millis()),
            "p95_ttft_ms": metrics.p95_ttft().map(|d: std::time::Duration| d.as_millis()),
            "median_total_time_ms": metrics.median_total_time().map(|d: std::time::Duration| d.as_millis()),
            "median_generation_speed_tps": metrics.median_generation_speed().map(|s| (s * 100.0).round() / 100.0),
            "priority_score": router.metrics().calculate_priority_score(&provider_name),
        });
        json_map.insert(provider_name, provider_json);
    }
    
    (StatusCode::OK, Json(serde_json::Value::Object(json_map))).into_response()
}

/// Dashboard endpoint - serves the web UI.
pub async fn dashboard() -> Response {
    let html = include_str!("../web/dashboard.html");
    (StatusCode::OK, axum::response::Html(html)).into_response()
}

/// Get all configured providers.
pub async fn get_providers(State(router): State<AppState>) -> Response {
    let config = router.config_snapshot().await;
    let providers = config.providers.clone();
    let discovered = config.discovered.clone();
    let mut json_map = serde_json::Map::new();
    
    for (name, config) in providers {
        // 使用 discovered 中的模型列表，如果没有则使用 capabilities
        let models = discovered.get(&name).cloned().unwrap_or_else(|| config.capabilities.clone());
        
        let provider_json = json!({
            "name": name,
            "base_url": config.base_url,
            "api_key": crate::config::mask_key(&config.api_key),
            "enabled": config.enabled,
            "priority": config.priority,
            "provider_type": config.provider_type,
            "connect_timeout_ms": config.connect_timeout_ms,
            "read_timeout_ms": config.read_timeout_ms,
            "ttft_timeout_ms": config.ttft_timeout_ms,
            "generation_timeout_ms": config.generation_timeout_ms,
            "total_timeout_ms": config.total_timeout_ms,
            "capabilities": models,
        });
        json_map.insert(name, provider_json);
    }
    
    (StatusCode::OK, Json(serde_json::Value::Object(json_map))).into_response()
}

/// Get discovered models for all providers.
pub async fn get_discovered_models(State(router): State<AppState>) -> Response {
    let config = router.config_snapshot().await;
    let discovered = config.discovered.clone();
    Json(discovered).into_response()
}

/// Clean orphaned discovered models (providers that no longer exist).
pub async fn clean_discovered_models(State(router): State<AppState>) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();
    
    let existing_providers: std::collections::HashSet<_> = config.providers.keys().cloned().collect();
    let original_count = config.discovered.len();
    
    config.discovered.retain(|k, _| existing_providers.contains(k));
    
    let removed_count = original_count - config.discovered.len();
    
    // 保存配置到文件
    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
    }
    
    // 重新加载配置到路由器
    router.reload_config(config).await;
    
    (StatusCode::OK, Json(json!({
        "removed": removed_count,
        "remaining": existing_providers.len()
    }))).into_response()
}

/// Get all model groups.
pub async fn get_model_groups(State(router): State<AppState>) -> Response {
    let config = router.config_snapshot().await;
    let models = config.models.clone();
    let discovered = config.discovered.clone();
    let mut json_map = serde_json::Map::new();
    
    // 1. 添加手动配置的模型组
    for (name, group) in models {
        let group_json = json!({
            "name": name,
            "providers": group.providers.iter().map(|p| json!({
                "provider": p.provider,
                "model_id": p.model_id,
            })).collect::<Vec<_>>(),
            "auto_generated": false,
        });
        json_map.insert(name, group_json);
    }
    
    // 2. 自动生成模型组（基于发现的模型）
    let auto_groups = auto_generate_model_groups(&discovered);
    for (name, group) in auto_groups {
        // 如果手动配置已存在，跳过自动生成
        if json_map.contains_key(&name) {
            continue;
        }
        
        let group_json = json!({
            "name": name,
            "providers": group.iter().map(|p| json!({
                "provider": p.provider,
                "model_id": p.model_id,
            })).collect::<Vec<_>>(),
            "auto_generated": true,
        });
        json_map.insert(name, group_json);
    }
    
    (StatusCode::OK, Json(serde_json::Value::Object(json_map))).into_response()
}

/// 自动生成模型组：按模型名称分组，忽略大小写
fn normalize_model_name(name: &str) -> String {
    // 去除方括号前缀，如 [AC混合]、[AG-特惠] 等
    let without_prefix = if name.starts_with('[') {
        if let Some(pos) = name.find(']') {
            name[pos + 1..].to_string()
        } else {
            name.to_string()
        }
    } else {
        name.to_string()
    };
    
    // 去除方括号后缀，如 [嵌入]、[缓存] 等
    let without_suffix = if without_prefix.ends_with(']') {
        if let Some(pos) = without_prefix.rfind('[') {
            without_prefix[..pos].to_string()
        } else {
            without_prefix
        }
    } else {
        without_prefix
    };
    
    // 转换为小写并去除空白
    let lower = without_suffix.to_lowercase().trim().to_string();
    
    // 版本号归一化：`.` 只出现在版本号中，统一替换为 `-`
    // 例如 "qwen3.8-27b" -> "qwen3-8-27b"，与 "Qwen3.8-27B" 归为同一组
    lower.replace('.', "-")
}

pub fn auto_generate_model_groups(discovered: &std::collections::HashMap<String, Vec<String>>)
    -> std::collections::HashMap<String, Vec<crate::config::ModelProviderRef>> 
{
    use std::collections::HashMap;
    
    // 按规范化模型名称分组
    let mut model_map: HashMap<String, Vec<crate::config::ModelProviderRef>> = HashMap::new();
    
    for (provider, models) in discovered {
        for model_id in models {
            // 规范化模型名称：去除方括号前缀，转小写，去除空格
            let normalized = normalize_model_name(model_id);
            
            let provider_ref = crate::config::ModelProviderRef {
                provider: provider.clone(),
                model_id: Some(model_id.clone()),
            };
            
            model_map.entry(normalized)
                .or_insert_with(Vec::new)
                .push(provider_ref);
        }
    }
    
    model_map
}

#[derive(serde::Deserialize)]
pub struct CreateApiKeyRequest {
    /// Optional custom key. Empty/absent ⇒ server generates `crsllm-<uuid>`.
    #[serde(default)]
    pub key: String,
}

#[derive(serde::Deserialize)]
pub struct CreateProviderRequest {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_read_timeout")]
    pub read_timeout_ms: u64,
    #[serde(default = "default_ttft_timeout")]
    pub ttft_timeout_ms: u64,
    #[serde(default = "default_generation_timeout")]
    pub generation_timeout_ms: u64,
    #[serde(default = "default_total_timeout")]
    pub total_timeout_ms: u64,
}

fn default_connect_timeout() -> u64 { 8000 }
fn default_read_timeout() -> u64 { 8000 }
fn default_ttft_timeout() -> u64 { 60000 }
fn default_generation_timeout() -> u64 { 90000 }
fn default_total_timeout() -> u64 { 240000 }

pub async fn create_provider(
    State(router): State<AppState>,
    Json(req): Json<CreateProviderRequest>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();
    
    if config.providers.contains_key(&req.name) {
        return (StatusCode::CONFLICT, format!("Provider '{}' already exists", req.name)).into_response();
    }
    
    let provider_config = crate::config::ProviderConfig {
        api_key: req.api_key,
        base_url: req.base_url,
        connect_timeout_ms: req.connect_timeout_ms,
        read_timeout_ms: req.read_timeout_ms,
        ttft_timeout_ms: req.ttft_timeout_ms,
        generation_timeout_ms: req.generation_timeout_ms,
        total_timeout_ms: req.total_timeout_ms,
        capabilities: Vec::new(),
        priority: None,
        provider_type: None,
        enabled: true,
    };
    
    config.providers.insert(req.name.clone(), provider_config);
    
    // 保存配置到文件
    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
    }
    
    // 重新加载配置到路由器
    router.reload_config(config).await;
    
    (StatusCode::CREATED, format!("Provider '{}' created", req.name)).into_response()
}

#[derive(serde::Deserialize)]
pub struct UpdateProviderRequest {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub connect_timeout_ms: Option<u64>,
    pub read_timeout_ms: Option<u64>,
    pub ttft_timeout_ms: Option<u64>,
    pub generation_timeout_ms: Option<u64>,
    pub total_timeout_ms: Option<u64>,
    pub priority: Option<i32>,
    pub provider_type: Option<String>,
    pub enabled: Option<bool>,
}

pub async fn update_provider(
    State(router): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<UpdateProviderRequest>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();
    
    let provider = match config.providers.get_mut(&name) {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, format!("Provider '{}' not found", name)).into_response(),
    };
    
    if let Some(base_url) = req.base_url {
        provider.base_url = base_url;
    }
    if let Some(api_key) = req.api_key {
        provider.api_key = api_key;
    }
    if let Some(v) = req.connect_timeout_ms {
        provider.connect_timeout_ms = v;
    }
    if let Some(v) = req.read_timeout_ms {
        provider.read_timeout_ms = v;
    }
    if let Some(v) = req.ttft_timeout_ms {
        provider.ttft_timeout_ms = v;
    }
    if let Some(v) = req.generation_timeout_ms {
        provider.generation_timeout_ms = v;
    }
    if let Some(v) = req.total_timeout_ms {
        provider.total_timeout_ms = v;
    }
    if let Some(v) = req.priority {
        provider.priority = Some(v);
    }
    if let Some(v) = req.provider_type {
        provider.provider_type = Some(v);
    }
    if let Some(v) = req.enabled {
        provider.enabled = v;
    }
    
    // 保存配置到文件
    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
    }
    
    // 重新加载配置到路由器
    router.reload_config(config).await;
    
    (StatusCode::OK, format!("Provider '{}' updated", name)).into_response()
}

#[derive(serde::Deserialize)]
pub struct SetProviderEnabledRequest {
    pub enabled: bool,
}

/// Toggle a provider on/off without touching other fields.
pub async fn set_provider_enabled(
    State(router): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<SetProviderEnabledRequest>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();

    let provider = match config.providers.get_mut(&name) {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, format!("Provider '{}' not found", name)).into_response(),
    };
    provider.enabled = req.enabled;

    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
    }
    router.reload_config(config).await;

    (StatusCode::OK, Json(json!({
        "message": format!("Provider '{}' {}", name, if req.enabled { "enabled" } else { "disabled" }),
        "enabled": req.enabled,
    }))).into_response()
}

pub async fn delete_provider(
    State(router): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();
    
    match config.providers.remove(&name) {
        Some(_) => {
            // 同时从 discovered 中移除该提供商的模型列表
            config.discovered.remove(&name);
            
            // 保存配置到文件
            if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
            }
            
            // 重新加载配置到路由器
            router.reload_config(config).await;
            
            (StatusCode::OK, format!("Provider '{}' deleted", name)).into_response()
        }
        None => (StatusCode::NOT_FOUND, format!("Provider '{}' not found", name)).into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct CreateModelGroupRequest {
    pub name: String,
    pub providers: Vec<crate::config::ModelProviderRef>,
}

pub async fn create_model_group(
    State(router): State<AppState>,
    Json(req): Json<CreateModelGroupRequest>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();
    
    if config.models.contains_key(&req.name) {
        return (StatusCode::CONFLICT, format!("Model group '{}' already exists", req.name)).into_response();
    }
    
    if req.providers.is_empty() {
        return (StatusCode::BAD_REQUEST, "Model group must contain at least one provider".to_string()).into_response();
    }
    for pr in &req.providers {
        if !config.providers.contains_key(&pr.provider) {
            return (StatusCode::BAD_REQUEST, format!("Provider '{}' not found", pr.provider)).into_response();
        }
    }
    
    let model_group = crate::config::ModelGroup {
        providers: req.providers,
    };
    
    config.models.insert(req.name.clone(), model_group);
    
    // 保存配置到文件
    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
    }
    
    // 重新加载配置到路由器
    router.reload_config(config).await;
    
    (StatusCode::CREATED, format!("Model group '{}' created", req.name)).into_response()
}

pub async fn delete_model_group(
    State(router): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();
    
    match config.models.remove(&name) {
        Some(_) => {
            // Remove from fallback chain if present
            config.fallback_chain.order.retain(|m| m != &name);
            
            // 保存配置到文件
            if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
            }
            
            // 重新加载配置到路由器
            router.reload_config(config).await;
            
            (StatusCode::OK, format!("Model group '{}' deleted", name)).into_response()
        }
        None => (StatusCode::NOT_FOUND, format!("Model group '{}' not found", name)).into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct UpdateModelGroupRequest {
    pub providers: Vec<crate::config::ModelProviderRef>,
}

/// Edit an existing model group's provider list (order = intra-group
/// failover priority). Validates every provider reference exists and is
/// non-empty.
pub async fn update_model_group(
    State(router): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<UpdateModelGroupRequest>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();

    if !config.models.contains_key(&name) {
        return (StatusCode::NOT_FOUND, format!("Model group '{}' not found", name)).into_response();
    }

    if req.providers.is_empty() {
        return (StatusCode::BAD_REQUEST, "Model group must contain at least one provider".to_string()).into_response();
    }

    for pr in &req.providers {
        if !config.providers.contains_key(&pr.provider) {
            return (StatusCode::BAD_REQUEST, format!("Provider '{}' not found", pr.provider)).into_response();
        }
    }

    config.models.insert(name.clone(), crate::config::ModelGroup { providers: req.providers });

    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
    }
    router.reload_config(config).await;

    (StatusCode::OK, format!("Model group '{}' updated", name)).into_response()
}

/// Get the global cross-group fallback chain order.
pub async fn get_fallback_chain(State(router): State<AppState>) -> Response {
    let config = router.config_snapshot().await;
    (StatusCode::OK, Json(json!({ "order": config.fallback_chain.order }))).into_response()
}

#[derive(serde::Deserialize)]
pub struct UpdateFallbackChainRequest {
    pub order: Vec<String>,
}

/// Set the global cross-group fallback chain. Every entry must resolve to a
/// manual or auto-generated model group.
pub async fn update_fallback_chain(
    State(router): State<AppState>,
    Json(req): Json<UpdateFallbackChainRequest>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();

    let auto_groups = auto_generate_model_groups(&config.discovered);
    for group_name in &req.order {
        if !config.models.contains_key(group_name) && !auto_groups.contains_key(group_name) {
            return (StatusCode::BAD_REQUEST, format!("Model group '{}' not found", group_name)).into_response();
        }
    }

    config.fallback_chain.order = req.order.clone();

    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
    }
    router.reload_config(config).await;

    (StatusCode::OK, Json(json!({
        "message": "Fallback chain updated",
        "order": req.order,
    }))).into_response()
}

/// Recent request log entries (newest first).
pub async fn get_logs(
    State(router): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Response {
    let limit = params.get("limit").and_then(|v| v.parse::<usize>().ok()).unwrap_or(200);
    let logs = router.metrics().recent_logs(limit);
    (StatusCode::OK, Json(json!({
        "logs": logs,
        "uptime_secs": router.metrics().uptime_secs(),
    }))).into_response()
}

/// Per-provider request history time series for trend charts.
pub async fn get_metrics_history(State(router): State<AppState>) -> Response {
    let history = router.metrics().get_history();
    (StatusCode::OK, Json(history)).into_response()
}

/// Get all routed models
pub async fn get_routed_models(State(router): State<AppState>) -> Response {
    let config = router.config_snapshot().await;
    let routed_models = &config.routed_models;
    Json(routed_models).into_response()
}

/// Create a new routed model
#[derive(Deserialize)]
pub struct CreateRoutedModelRequest {
    pub name: String,
    pub chain: Vec<String>,
}

pub async fn create_routed_model(
    State(router): State<AppState>,
    Json(req): Json<CreateRoutedModelRequest>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();
    
    if config.routed_models.contains_key(&req.name) {
        return (StatusCode::CONFLICT, format!("Routed model '{}' already exists", req.name)).into_response();
    }
    
    // Validate that all model groups in chain exist (check both manual and auto-generated)
    let discovered = config.discovered.clone();
    let auto_groups = auto_generate_model_groups(&discovered);
    
    for group_name in &req.chain {
        if !config.models.contains_key(group_name) && !auto_groups.contains_key(group_name) {
            return (StatusCode::BAD_REQUEST, format!("Model group '{}' not found", group_name)).into_response();
        }
    }
    
    let routed_model = crate::config::RoutedModel {
        name: req.name.clone(),
        chain: req.chain,
    };
    
    config.routed_models.insert(req.name.clone(), routed_model);
    
    // 保存配置到文件
    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
    }
    
    // 重新加载配置到路由器
    router.reload_config(config).await;
    
    (StatusCode::CREATED, format!("Routed model '{}' created", req.name)).into_response()
}

/// Update a routed model
#[derive(Deserialize)]
pub struct UpdateRoutedModelRequest {
    pub chain: Vec<String>,
}

pub async fn update_routed_model(
    State(router): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<UpdateRoutedModelRequest>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();
    
    if !config.routed_models.contains_key(&name) {
        return (StatusCode::NOT_FOUND, format!("Routed model '{}' not found", name)).into_response();
    }
    
    // Validate that all model groups in chain exist (check both manual and auto-generated)
    let discovered = config.discovered.clone();
    let auto_groups = auto_generate_model_groups(&discovered);
    
    for group_name in &req.chain {
        if !config.models.contains_key(group_name) && !auto_groups.contains_key(group_name) {
            return (StatusCode::BAD_REQUEST, format!("Model group '{}' not found", group_name)).into_response();
        }
    }
    
    let routed_model = crate::config::RoutedModel {
        name: name.clone(),
        chain: req.chain,
    };
    
    config.routed_models.insert(name.clone(), routed_model);
    
    // 保存配置到文件
    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
    }
    
    // 重新加载配置到路由器
    router.reload_config(config).await;
    
    (StatusCode::OK, format!("Routed model '{}' updated", name)).into_response()
}

/// Delete a routed model
pub async fn delete_routed_model(
    State(router): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();
    
    match config.routed_models.remove(&name) {
        Some(_) => {
            // 保存配置到文件
            if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
            }
            
            // 重新加载配置到路由器
            router.reload_config(config).await;
            
            (StatusCode::OK, format!("Routed model '{}' deleted", name)).into_response()
        }
        None => (StatusCode::NOT_FOUND, format!("Routed model '{}' not found", name)).into_response(),
    }
}

/// Discover available models from all configured providers.
pub async fn list_models(State(router): State<AppState>) -> Response {
    let results = router.discover_models().await;
    let mut json_map = serde_json::Map::new();
    for (provider, models) in &results {
        json_map.insert(provider.clone(), serde_json::json!(models));
    }
    (StatusCode::OK, Json(serde_json::Value::Object(json_map))).into_response()
}

/// 重新扫描所有提供商的模型列表
pub async fn refresh_models(State(router): State<AppState>) -> Response {
    let results = router.discover_models().await;
    
    let total_providers = results.len();
    let total_models: usize = results.values().map(|m| m.len()).sum();
    
    (StatusCode::OK, Json(json!({
        "message": "模型列表刷新完成",
        "providers": total_providers,
        "total_models": total_models,
        "details": results.iter().map(|(p, m)| json!({
            "provider": p,
            "model_count": m.len()
        })).collect::<Vec<_>>()
    }))).into_response()
}

/// 刷新单个提供商的模型列表
pub async fn refresh_provider_models(
    State(router): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    let config = router.config_snapshot().await;
    
    let provider_config = match config.provider(&name) {
        Some(p) => p.clone(),
        None => return (StatusCode::NOT_FOUND, format!("Provider '{}' not found", name)).into_response(),
    };
    
    // 创建适配器并获取模型列表
    let adapter = router.create_registry_adapter(&name, provider_config, name.clone());
    match adapter.list_models().await {
        Ok(models) => {
            // 更新配置中的 discovered 字段
            let mut config = config.as_ref().clone();
            config.discovered.insert(name.clone(), models.clone());
            
            // 保存配置到文件
            if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
            }
            
            // 重新加载配置到路由器
            router.reload_config(config).await;
            
            (StatusCode::OK, Json(json!({
                "message": format!("Provider '{}' models refreshed", name),
                "model_count": models.len(),
                "models": models
            }))).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to refresh models: {}", e)).into_response()
        }
    }
}

/// 手动配置提供商的模型列表
pub async fn update_provider_models(
    State(router): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<UpdateProviderModelsRequest>,
) -> Response {
    let mut config = router.config_snapshot().await.as_ref().clone();
    
    if !config.providers.contains_key(&name) {
        return (StatusCode::NOT_FOUND, format!("Provider '{}' not found", name)).into_response();
    }
    
    // 更新 discovered 字段
    config.discovered.insert(name.clone(), req.models.clone());
    
    // 保存配置到文件
    if let Err(e) = crate::config::save_config(&config, "config/crsllm.toml") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)).into_response();
    }
    
    // 重新加载配置到路由器
    router.reload_config(config).await;
    
    (StatusCode::OK, Json(json!({
        "message": format!("Provider '{}' models updated", name),
        "model_count": req.models.len(),
        "models": req.models
    }))).into_response()
}

/// 测试提供商连通性
pub async fn test_provider(
    State(router): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    let config = router.config_snapshot().await;
    
    let provider_config = match config.provider(&name) {
        Some(p) => p.clone(),
        None => return (StatusCode::NOT_FOUND, format!("Provider '{}' not found", name)).into_response(),
    };
    
    // 测试连通性：尝试获取模型列表
    let adapter = router.create_registry_adapter(&name, provider_config, name.clone());
    let start = std::time::Instant::now();
    
    match adapter.list_models().await {
        Ok(models) => {
            let latency_ms = start.elapsed().as_millis();
            (StatusCode::OK, Json(json!({
                "status": "connected",
                "latency_ms": latency_ms,
                "model_count": models.len(),
                "sample_models": models.iter().take(5).collect::<Vec<_>>()
            }))).into_response()
        }
        Err(e) => {
            let latency_ms = start.elapsed().as_millis();
            (StatusCode::OK, Json(json!({
                "status": "error",
                "latency_ms": latency_ms,
                "error": e.to_string()
            }))).into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct UpdateProviderModelsRequest {
    pub models: Vec<String>,
}

/// Anthropic Messages endpoint (primary).
pub async fn anthropic_messages(
    State(router): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Response {
    // Parse the request as our unified internal format
    let req: crate::translator::AnthropicRequest = match serde_json::from_value::<crate::translator::AnthropicRequest>(payload) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "invalid_request_error",
                        "message": format!("Invalid request: {}", e)
                    }
                })),
            )
                .into_response();
        }
    };

    let model_group = req.model.clone();

    // Check if client wants streaming
    let accept = headers.get("accept").and_then(|v: &HeaderValue| v.to_str().ok()).unwrap_or("");
    let stream = headers.get("x-crsllm-stream").and_then(|v: &HeaderValue| v.to_str().ok());

    if accept.contains("text/event-stream") || stream == Some("true") {
        stream_response(router, &req, &model_group).await
    } else {
        non_streaming_response(&router, &req, &model_group).await
    }
}

/// OpenAI chat completions endpoint (translated to Anthropic internally).
pub async fn openai_chat_completions(
    State(router): State<AppState>,
    _headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Response {
    // Translate OpenAI → Anthropic
    let anthropic_req = openai_to_anthropic(&payload);
    let model_group = anthropic_req.model.clone();

    // OpenAI streaming uses `stream: true` in the body
    let want_stream = payload.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    if want_stream {
        stream_response(router, &anthropic_req, &model_group).await
    } else {
        non_streaming_response(&router, &anthropic_req, &model_group).await
    }
}

/// Non-streaming response: route, translate, return JSON.
async fn non_streaming_response(
    router: &Router,
    req: &crate::translator::AnthropicRequest,
    model_group: &str,
) -> Response {
    match router.route(model_group, req).await {
        Ok(resp) => (StatusCode::OK, Json(serde_json::to_value(&resp).unwrap_or_default())).into_response(),
        Err(e) => {
            let status = e.status.unwrap_or(503);
            let code = StatusCode::from_u16(status.min(599)).unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
            (
                code,
                Json(translator::anthropic_error(&e.body)),
            )
                .into_response()
        }
    }
}

/// Streaming response: use the SSE failover stream, convert to the
/// appropriate format for the client.
async fn stream_response(
    router: Arc<Router>,
    req: &crate::translator::AnthropicRequest,
    _model_group: &str,
) -> Response {
    // Streaming shares the Router's config snapshot + metrics store with the
    // non-streaming path, so routed chains and per-request stats stay consistent.
    let req_owned = req.clone();
    let stream = streaming::stream_with_fallback(router, req_owned.model.clone(), req_owned);

    let body = axum::body::Body::from_stream(stream);
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        "content-type",
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    response.headers_mut().insert(
        "cache-control",
        HeaderValue::from_static("no-cache"),
    );
    response
}

/// Translate an OpenAI chat completions request to Anthropic format.
fn openai_to_anthropic(openai_req: &serde_json::Value) -> crate::translator::AnthropicRequest {
    let model = openai_req
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let messages_val = openai_req.get("messages").cloned().unwrap_or(json!([]));

    let mut anthropic_messages = Vec::new();
    if let Some(arr) = messages_val.as_array() {
        for msg in arr {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let content = if let Some(content_val) = msg.get("content") {
                if let Some(text) = content_val.as_str() {
                    crate::translator::AnthropicContent::Text(text.to_string())
                } else if let Some(arr) = content_val.as_array() {
                    let mut blocks = Vec::new();
                    for block in arr {
                        if let Some(btype) = block.get("type").and_then(|v| v.as_str()) {
                            match btype {
                                "text" => {
                                    if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                                        blocks.push(crate::translator::AnthropicContentBlock::Text {
                                            text: t.to_string(),
                                            citations: None,
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    crate::translator::AnthropicContent::Blocks(blocks)
                } else {
                    crate::translator::AnthropicContent::Text(content_val.to_string())
                }
            } else {
                crate::translator::AnthropicContent::Text(String::new())
            };

            anthropic_messages.push(crate::translator::AnthropicMessage {
                role: role.to_string(),
                content: Some(content),
                name: msg.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
            });
        }
    }

    crate::translator::AnthropicRequest {
        model,
        messages: anthropic_messages,
        system: openai_req
            .get("system")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        max_tokens: openai_req.get("max_tokens").and_then(|v| v.as_u64()).map(|u| u as u32),
        temperature: openai_req.get("temperature").and_then(|v| v.as_f64()),
        top_p: openai_req.get("top_p").and_then(|v| v.as_f64()),
        top_k: openai_req.get("top_k").and_then(|v| v.as_u64()).map(|u| u as u32),
        tools: vec![],
        tool_choice: None,
        stop_sequences: vec![],
    }
}
