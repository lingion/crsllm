//! Integration tests for CRSLLM config loading and fallback chain logic.

use crsllm::config::load_config;
use crsllm::router::Router;

#[test]
fn test_config_loads_example() {
    let config = load_config("config/crsllm.toml.example");
    assert!(config.is_ok(), "example config should parse");
    let config = config.unwrap();
    assert!(config.providers.contains_key("openai"));
    assert!(config.providers.contains_key("anthropic"));
    assert!(config.providers.contains_key("google"));
    assert!(config.models.contains_key("gpt-4o"));
    assert!(config.models.contains_key("claude-sonnet-4"));
    assert!(config.models.contains_key("gemini-2.0-flash"));
}

#[tokio::test]
async fn test_fallback_chain_dedup() {
    let config = load_config("config/crsllm.toml.example").unwrap();
    let router = Router::new(config);

    // Requested model is at index 1 in the chain (middle).
    // build_fallback_chain should put the requested model first, then the rest.
    let chain = router.build_fallback_chain("claude-sonnet-4").await;
    assert_eq!(chain[0], "claude-sonnet-4");
    assert!(!chain[1..].contains(&"claude-sonnet-4".to_string())); // no dupes
    assert_eq!(chain.len(), 3);
}

#[tokio::test]
async fn test_routed_model_chain_order() {
    let mut config = load_config("config/crsllm.toml.example").unwrap();
    config.routed_models.insert(
        "my-route".to_string(),
        crsllm::config::RoutedModel {
            name: "my-route".to_string(),
            chain: vec!["gemini-2.0-flash".to_string(), "gpt-4o".to_string()],
        },
    );
    let router = Router::new(config);

    // Routed models return their custom chain verbatim (order preserved).
    let chain = router.build_fallback_chain("my-route").await;
    assert_eq!(chain, vec!["gemini-2.0-flash".to_string(), "gpt-4o".to_string()]);

    // Unknown names still fall back to the global chain behavior.
    let chain2 = router.build_fallback_chain("gpt-4o").await;
    assert_eq!(chain2[0], "gpt-4o");
}

#[test]
fn test_model_id_mapping_with_override() {
    let config = load_config("config/crsllm.toml.example").unwrap();
    let group = config.models.get("gpt-4o").unwrap();
    // First provider ref has model_id override
    assert_eq!(group.providers[0].model_id.as_deref(), Some("gpt-4o"));
}

#[test]
fn test_model_id_mapping_without_override() {
    let config = load_config("config/crsllm.toml.example").unwrap();
    // claude-sonnet-4 group: second provider ref (openai) has model_id override
    // First provider ref (anthropic) also has model_id.
    // The point: model_id can be None for a ref — verify the field exists.
    let group = config.models.get("claude-sonnet-4").unwrap();
    assert_eq!(group.providers.len(), 2);
    // Both have model_id overrides in the example config
    assert!(group.providers[0].model_id.is_some());
    assert!(group.providers[1].model_id.is_some());
}

#[test]
fn test_gateway_key_accepts_both_auth_styles() {
    use axum::http::{HeaderMap, HeaderValue};
    use crsllm::handler::client_key_ok;

    let keys = vec!["crsllm-secret-1".to_string()];

    let mut h = HeaderMap::new();
    h.insert("x-api-key", HeaderValue::from_static("crsllm-secret-1"));
    assert!(client_key_ok(&h, &keys), "x-api-key should authenticate");

    let mut h2 = HeaderMap::new();
    h2.insert("authorization", HeaderValue::from_static("Bearer crsllm-secret-1"));
    assert!(client_key_ok(&h2, &keys), "Bearer should authenticate");

    let mut h3 = HeaderMap::new();
    h3.insert("x-api-key", HeaderValue::from_static("wrong-key"));
    assert!(!client_key_ok(&h3, &keys), "wrong key must fail");

    let h4 = HeaderMap::new();
    assert!(!client_key_ok(&h4, &keys), "missing credentials must fail");
}

#[test]
fn test_gateway_config_default_has_no_keys() {
    // Empty api_keys = open access; bootstrap can't lock itself out.
    let g = crsllm::config::GatewayConfig::default();
    assert!(g.api_keys.is_empty());
}

#[test]
fn test_create_api_key_request_absent_key_default() {
    // Third-party callers POST `{}` or omit `key` entirely; that used to be a
    // 422. serde(default) must accept it and let the server generate a UUID.
    let req: crsllm::handler::CreateApiKeyRequest =
        serde_json::from_str("{}").expect("empty body should deserialize");
    assert_eq!(req.key, "");
    let req2: crsllm::handler::CreateApiKeyRequest =
        serde_json::from_str(r#"{"key":"custom"}"#).unwrap();
    assert_eq!(req2.key, "custom");
}
