use std::collections::HashMap;

/// Normalize model ID to lowercase for comparison.
/// Dots are preserved: alias table keys and upstream model IDs (e.g.
/// `qwen3.8-flash-next`) contain version dots and must not be rewritten.
pub fn normalize_model_id(model_id: &str) -> String {
    model_id.to_lowercase()
}

/// Check if two model IDs refer to the same model (case-insensitive)
pub fn is_same_model(id1: &str, id2: &str) -> bool {
    normalize_model_id(id1) == normalize_model_id(id2)
}

/// Model alias mapping for common variations
pub fn get_canonical_model_id(model_id: &str) -> String {
    let normalized = normalize_model_id(model_id);
    
    // Common aliases
    let aliases: HashMap<&str, &str> = [
        // Qwen variations
        ("qwen3.8-flash-next", "qwen3.8-flash-next"),
        ("qwen3.8-max", "qwen3.8-max"),
        ("qwen3.8-27b", "qwen3.8-27b"),
        
        // Claude variations
        ("claude-opus-5", "claude-opus-5"),
        ("claude-opus-4-8", "claude-opus-4-8"),
        ("claude-3-5-sonnet", "claude-3-5-sonnet"),
        
        // GPT variations
        ("gpt-5.6-terra", "gpt-5.6-terra"),
        ("gpt-5.6-luna", "gpt-5.6-luna"),
        ("gpt-4", "gpt-4"),
        
        // DeepSeek variations
        ("deepseek-v4-pro", "deepseek-v4-pro"),
        ("deepseek-v4-flash", "deepseek-v4-flash"),
        ("deepseek-v4.1-flash", "deepseek-v4.1-flash"),
        
        // GLM variations
        ("glm-5.3", "glm-5.3"),
        ("glm-5.3-flash", "glm-5.3-flash"),
        ("glm-5.2", "glm-5.2"),
        
        // Kimi variations
        ("kimi-k3", "kimi-k3"),
        ("kimi-k2.6", "kimi-k2.6"),
    ].iter().cloned().collect();
    
    aliases.get(normalized.as_str())
        .map(|s| s.to_string())
        .unwrap_or(normalized)
}

/// Find all providers that support a given model (case-insensitive)
pub fn find_providers_for_model(
    discovered_models: &HashMap<String, Vec<String>>,
    target_model: &str,
) -> Vec<(String, String)> {
    let target_normalized = normalize_model_id(target_model);
    let mut results = Vec::new();
    
    for (provider_name, models) in discovered_models {
        for model in models {
            if normalize_model_id(model) == target_normalized {
                results.push((provider_name.clone(), model.clone()));
                break;
            }
        }
    }
    
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_normalize_model_id() {
        assert_eq!(normalize_model_id("Qwen3.8-Flash-Next"), "qwen3.8-flash-next");
        assert_eq!(normalize_model_id("GPT-4"), "gpt-4");
        assert_eq!(normalize_model_id("Claude-Opus-5"), "claude-opus-5");
    }
    
    #[test]
    fn test_is_same_model() {
        assert!(is_same_model("Qwen3.8-Flash-Next", "qwen3.8-flash-next"));
        assert!(is_same_model("GPT-4", "gpt-4"));
        assert!(!is_same_model("gpt-4", "gpt-5"));
    }
    
    #[test]
    fn test_get_canonical_model_id() {
        assert_eq!(get_canonical_model_id("Qwen3.8-Flash-Next"), "qwen3.8-flash-next");
        assert_eq!(get_canonical_model_id("GPT-5.6-TERRA"), "gpt-5.6-terra");
    }
}
