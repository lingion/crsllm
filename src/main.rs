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

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP_TEXT: &str = r#"crsllm — Cross-provider Resilient Smart LLM Gateway

USAGE:
    crsllm [OPTIONS]

OPTIONS:
    -h, --help                 Show this help message
    -V, --version              Show version information
    -p, --port <PORT>          Port to listen on (default: 3002)
    -c, --config <PATH>        Config file path (default: config/crsllm.toml)
    --init                     Initialize config from example template
    --discover-models          Discover available models from all providers

EXAMPLES:
    crsllm                     Start with default config on port 3002
    crsllm --port 8080         Start on custom port
    crsllm --init              Create config/crsllm.toml from template
    crsllm --discover-models   Fetch models from all configured providers

ENDPOINTS:
    POST /v1/messages          Anthropic Messages API
    POST /v1/chat/completions  OpenAI Chat Completions API
    POST /v1/responses         OpenAI Responses API
    GET  /v1/models            List available models
    GET  /health               Health check
    GET  /dashboard            Web dashboard
"#;

/// Find the example config template, checking next to the target path first,
/// then falling back to the project's config/ directory.
fn find_example_template(config_path: &str) -> Option<String> {
    // 1. Next to the target: config/crsllm.toml.example
    let sibling = config_path.replace(".toml", ".toml.example");
    if std::path::Path::new(&sibling).exists() {
        return Some(sibling);
    }
    // 2. Project config directory: config/crsllm.toml.example
    let project = "config/crsllm.toml.example";
    if std::path::Path::new(project).exists() {
        return Some(project.to_string());
    }
    None
}

/// Ensure config/crsllm.toml exists; if not, copy from .example template.
fn ensure_config(config_path: &str) -> Result<(), String> {
    if std::path::Path::new(config_path).exists() {
        return Ok(());
    }
    let example = find_example_template(config_path).ok_or_else(|| {
        format!("Config not found: {}\nRun `crsllm --init` or create it manually.", config_path)
    })?;
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(config_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }
    std::fs::copy(&example, config_path)
        .map_err(|e| format!("Failed to create config from template: {}", e))?;
    println!("✓ Created {} from template", config_path);
    println!("  Edit it to add your API keys, then run `crsllm` again.");
    Err(format!("Config initialized at {}. Please edit it and restart.", config_path))
}

/// Handle --init: copy example config to real path.
fn init_config(config_path: &str) {
    if std::path::Path::new(config_path).exists() {
        println!("Config already exists: {}", config_path);
        return;
    }
    let example = match find_example_template(config_path) {
        Some(e) => e,
        None => {
            eprintln!("Template not found: config/crsllm.toml.example");
            std::process::exit(1);
        }
    };
    if let Some(parent) = std::path::Path::new(config_path).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("Failed to create directory: {}", e);
            std::process::exit(1);
        }
    }
    match std::fs::copy(&example, config_path) {
        Ok(_) => {
            println!("✓ Created {}", config_path);
            println!("  Edit it to add your API keys, then run `crsllm`.");
        }
        Err(e) => {
            eprintln!("Failed to create config: {}", e);
            std::process::exit(1);
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();

    // Handle --help / -h
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{}", HELP_TEXT);
        return;
    }

    // Handle --version / -V
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("crsllm {}", VERSION);
        return;
    }

    // Resolve config path from args
    let config_path = args.iter().position(|a| a == "--config" || a == "-c")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "config/crsllm.toml".to_string());

    // Handle --init
    if args.iter().any(|a| a == "--init") {
        init_config(&config_path);
        return;
    }

    // CLI: --discover-models prints discovered models from all providers
    if args.iter().any(|a| a == "--discover-models") {
        if let Err(e) = ensure_config(&config_path) {
            eprintln!("{}", e);
            std::process::exit(1);
        }

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

    // Auto-create config from template if missing
    if let Err(e) = ensure_config(&config_path) {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    let mut config = config::load_config(&config_path).expect("Failed to load config");
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

    let port: u16 = args.iter().position(|a| a == "--port" || a == "-p")
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
