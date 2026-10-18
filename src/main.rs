#![allow(dead_code)]

pub mod adapters;
pub mod config;
pub mod error;
pub mod handler;
pub mod metrics;
pub mod model_resolver;
pub mod responses;
pub mod router;
pub mod streaming;
pub mod translator;

use axum::{
    middleware::from_fn_with_state,
    routing::{get, post, put, patch, delete},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // CLI: --discover-models prints discovered models from all providers
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--discover-models" {
        let config_path = if args.len() > 2 && args[2] == "--config" {
            args.get(3).cloned().unwrap_or_else(|| "config/crsllm.toml".to_string())
        } else {
            "config/crsllm.toml".to_string()
        };

        let mut config = config::load_config(&config_path).expect("Failed to load config");
        let registry = config::load_registry("config/providers.toml").expect("Failed to load registry");
        config.merge_registry(&registry);
        let router = router::Router::new_with_registry(config.clone(), registry);
        let results = router.discover_models().await;

        // Update config with discovered models
        for (name, models) in &results {
            config.discovered.insert(name.clone(), models.clone());
        }

        // Save discovered models to config file
        let output_path = config_path.replace(".toml", ".discovered.toml");
        if let Err(e) = router::Router::save_discovered_models(&config, &output_path) {
            eprintln!("Failed to save discovered models: {}", e);
        } else {
            println!("✓ Saved discovered models to {}", output_path);
        }

        println!("=== Discovered Models ===");
        for (provider, models) in &results {
            println!("\n{}: {} models", provider, models.len());
            for m in models {
                println!("  - {}", m);
            }
        }
        println!();
        return;
    }

    let mut config = config::load_config("config/crsllm.toml").expect("Failed to load config");
    let registry = config::load_registry("config/providers.toml").expect("Failed to load registry");
    config.merge_registry(&registry);
    
    // discovered数据已经在crsllm.toml中，不需要从单独文件加载
    tracing::info!("Loaded {} discovered models from config", config.discovered.len());
    
    // Validate provider base URLs and warn about potential issues
    for (name, provider) in &config.providers {
        let base_url = &provider.base_url;
        if base_url.ends_with("/v1") || base_url.ends_with("/v1/") {
            tracing::warn!(
                "Provider '{}' base_url ends with /v1: {}. This may cause doubled /v1 in API calls. Consider removing /v1 from base_url.",
                name,
                base_url
            );
        }
    }

    let router = Arc::new(router::Router::new_with_registry(config, registry));

    let app = Router::new()
        .route("/health", get(handler::health))
        .route("/metrics", get(handler::get_metrics))
        .route("/dashboard", get(handler::dashboard))
        .route("/api/api-keys", get(handler::get_api_keys).post(handler::create_api_key))
        .route("/api/api-keys/:key", delete(handler::delete_api_key))
        .route("/api/providers", get(handler::get_providers).post(handler::create_provider))
        .route("/api/providers/:name", put(handler::update_provider).delete(handler::delete_provider))
        .route("/api/providers/:name/enabled", patch(handler::set_provider_enabled))
        .route("/api/discovered-models", get(handler::get_discovered_models))
        .route("/api/clean-discovered", post(handler::clean_discovered_models))
        .route("/api/model-groups", get(handler::get_model_groups).post(handler::create_model_group))
        .route("/api/model-groups/:name", put(handler::update_model_group).delete(handler::delete_model_group))
        .route("/api/fallback-chain", get(handler::get_fallback_chain).put(handler::update_fallback_chain))
        .route("/api/logs", get(handler::get_logs))
        .route("/api/metrics/history", get(handler::get_metrics_history))
        .route("/api/routed-models", get(handler::get_routed_models).post(handler::create_routed_model))
        .route("/api/routed-models/:name", put(handler::update_routed_model).delete(handler::delete_routed_model))
        .route("/api/refresh-models", post(handler::refresh_models))
        .route("/api/providers/:name/refresh", post(handler::refresh_provider_models))
        .route("/api/providers/:name/models", put(handler::update_provider_models))
        .route("/api/providers/:name/test", post(handler::test_provider))
        .route("/v1/messages", post(handler::anthropic_messages))
        .route("/v1/chat/completions", post(handler::openai_chat_completions))
        .route("/v1/responses", post(handler::openai_responses))
        .route("/v1/models", get(handler::list_models))
        .with_state(router.clone())
        .layer(from_fn_with_state(router, handler::gateway_auth));

    let port: u16 = args.iter().position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse().ok())
        .unwrap_or(3002);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("CRSLLM listening on {}", addr);
    axum::serve(
        tokio::net::TcpListener::bind(addr).await.unwrap(),
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
