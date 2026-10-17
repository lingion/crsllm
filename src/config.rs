use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Provider configuration for a specific key/endpoint.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ProviderConfig {
    pub api_key: String,
    pub base_url: String,
    #[serde(default = "default_timeout_connect_ms")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_timeout_read_ms")]
    pub read_timeout_ms: u64,
    /// Time to first token timeout (ms)
    #[serde(default = "default_ttft_timeout_ms")]
    pub ttft_timeout_ms: u64,
    /// Max interval between tokens (ms)
    #[serde(default = "default_generation_timeout_ms")]
    pub generation_timeout_ms: u64,
    /// Total request budget (ms)
    #[serde(default = "default_total_timeout_ms")]
    pub total_timeout_ms: u64,
    /// Provider priority (lower = higher priority)
    #[serde(default)]
    pub priority: Option<i32>,
    /// Provider type: "free", "paid", "custom"
    #[serde(default)]
    pub provider_type: Option<String>,
    /// Capabilities this provider supports (e.g. "vision", "search", "tools").
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Whether this provider participates in routing. Disabled providers are
    /// skipped by both the non-streaming and streaming paths.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Mask an API key for safe display: keep the first 6 and last 4 characters.
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 10 {
        return "***".to_string();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{}***{}", head, tail)
}

fn default_timeout_connect_ms() -> u64 {
    8_000
}

fn default_timeout_read_ms() -> u64 {
    8_000
}

fn default_ttft_timeout_ms() -> u64 {
    60_000
}

fn default_generation_timeout_ms() -> u64 {
    90_000
}

fn default_total_timeout_ms() -> u64 {
    240_000
}

/// A provider entry within a model group. References a provider config
/// by name and optionally overrides the model ID.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ModelProviderRef {
    pub provider: String,
    /// Override the provider's default model ID for this model group.
    #[serde(default)]
    pub model_id: Option<String>,
}

/// A model group: groups providers that serve the same logical model.
/// Intra-group fallback iterates these providers in order.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ModelGroup {
    pub providers: Vec<ModelProviderRef>,
}

/// Cross-model-group fallback chain.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct FallbackChain {
    pub order: Vec<String>,
}

/// A routed model with custom fallback chain.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RoutedModel {
    /// Display name for this routed model
    pub name: String,
    /// Ordered list of model groups to try in sequence
    pub chain: Vec<String>,
}

/// Global timeout configuration.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Timeouts {
    #[serde(default = "default_sse_inactivity_secs")]
    pub sse_inactivity_secs: u64,
    #[serde(default = "default_deadline_secs")]
    pub deadline_secs: u64,
}

fn default_sse_inactivity_secs() -> u64 {
    90
}

fn default_deadline_secs() -> u64 {
    120
}

/// Registry entry describing a known provider's base URL and discovery info.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ProviderRegistryEntry {
    pub base_url: String,
    #[serde(rename = "auth_type")]
    pub auth_type: String, // "bearer" | "api_key" | "query"
    #[serde(rename = "models_path")]
    pub models_path: String,
    pub label: String,
}

/// Bundled provider registry loaded from providers.toml.
pub type ProviderRegistry = HashMap<String, ProviderRegistryEntry>;

/// Registry TOML structure
#[derive(Deserialize, Default)]
struct RegistryFile {
    #[serde(default)]
    registry: ProviderRegistry,
}

/// Gateway-level settings: client API keys callers must present.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct GatewayConfig {
    /// API keys accepted in `x-api-key` / `Authorization` headers.
    #[serde(default)]
    pub api_keys: Vec<String>,
}

/// Top-level config loaded from TOML.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Config {
    /// Gateway API keys for authentication
    #[serde(default)]
    pub gateway: GatewayConfig,

    /// Named provider configs (api keys, base URLs).
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,

    /// Named model groups (logical model → list of provider refs).
    #[serde(default)]
    pub models: HashMap<String, ModelGroup>,

    /// Routed models with custom fallback chains.
    #[serde(default)]
    pub routed_models: HashMap<String, RoutedModel>,

    /// Cross-model-group fallback chain.
    #[serde(default)]
    pub fallback_chain: FallbackChain,

    /// Timeout settings.
    #[serde(default)]
    pub timeouts: Timeouts,

    /// Auto-discovered model cache (populated at startup or on demand).
    #[serde(default)]
    pub discovered: HashMap<String, Vec<String>>,
}

impl Config {
    /// Look up all providers for a model group by logical name.
    pub fn model_group(&self, name: &str) -> Option<&ModelGroup> {
        self.models.get(name)
    }

    /// Look up a routed model by name.
    pub fn routed_model(&self, name: &str) -> Option<&RoutedModel> {
        self.routed_models.get(name)
    }

    /// Look up a provider config by name.
    pub fn provider(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }

    /// Get the SSE inactivity timeout as a Duration.
    pub fn sse_inactivity(&self) -> Duration {
        Duration::from_secs(self.timeouts.sse_inactivity_secs)
    }

    /// Get the overall deadline as a Duration.
    pub fn deadline(&self) -> Duration {
        Duration::from_secs(self.timeouts.deadline_secs)
    }

    /// Resolve the actual model ID for a provider model ref.
    /// Falls back to the model group name if no model_id override is set.
    pub fn resolve_model_id(&self, group_name: &str, provider_name: &str, mpr: &ModelProviderRef) -> String {
        if let Some(mid) = &mpr.model_id {
            return mid.clone();
        }
        self.models
            .get(group_name)
            .and_then(|g| {
                g.providers.iter()
                    .find(|p| p.provider == *provider_name)
                    .and_then(|p| p.model_id.clone())
            })
            .unwrap_or_else(|| group_name.to_string())
    }

    /// Merge the provider registry with the user's config.
    /// This fills in base_url + auth_type for any provider in the user config
    /// that references a registry entry, and also adds any registry provider
    /// that has an api_key in the user config.
    pub fn merge_registry(&mut self, registry: &ProviderRegistry) {
        for (name, entry) in registry {
            // If user already defined this provider with an api_key,
            // fill in the base_url from the registry if not set.
            if let Some(pc) = self.providers.get_mut(name) {
                if pc.base_url.is_empty() {
                    pc.base_url = entry.base_url.clone();
                }
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            gateway: Default::default(),
            providers: Default::default(),
            models: Default::default(),
            routed_models: Default::default(),
            fallback_chain: FallbackChain::default(),
            timeouts: Timeouts::default(),
            discovered: Default::default(),
        }
    }
}

impl Default for FallbackChain {
    fn default() -> Self {
        FallbackChain { order: vec![] }
    }
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            sse_inactivity_secs: 90,
            deadline_secs: 120,
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        ProviderConfig {
            api_key: String::new(),
            base_url: String::new(),
            connect_timeout_ms: default_timeout_connect_ms(),
            read_timeout_ms: default_timeout_read_ms(),
            ttft_timeout_ms: default_ttft_timeout_ms(),
            generation_timeout_ms: default_generation_timeout_ms(),
            total_timeout_ms: default_total_timeout_ms(),
            priority: None,
            provider_type: None,
            capabilities: vec![],
            enabled: true,
        }
    }
}

impl Default for ModelGroup {
    fn default() -> Self {
        ModelGroup {
            providers: vec![],
        }
    }
}

impl Default for ModelProviderRef {
    fn default() -> Self {
        ModelProviderRef {
            provider: String::new(),
            model_id: None,
        }
    }
}

/// Load the bundled provider registry from providers.toml.
pub fn load_registry(path: &str) -> Result<ProviderRegistry, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
    let registry_file: RegistryFile = toml::from_str(&contents).map_err(ConfigError::Parse)?;
    Ok(registry_file.registry)
}

pub fn load_config(path: &str) -> Result<Config, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
    let contents = dedup_toml_tables(&contents);
    let config: Config = toml::from_str(&contents).map_err(ConfigError::Parse)?;
    Ok(config)
}

/// Drop duplicate key assignments across repeated `[table]` blocks.
///
/// Hand-edited configs can end up with the same simple table declared twice
/// (e.g. a legacy `[timeouts]` block plus a console-written one), which is
/// invalid TOML and made every fresh boot panic. Duplicate top-level table
/// blocks are merged into their first occurrence (first assignment of each
/// key wins); anything else is returned untouched so the parser still
/// reports real errors honestly.
pub fn dedup_toml_tables(contents: &str) -> String {
    if toml::from_str::<toml::Value>(contents).is_ok() {
        return contents.to_string();
    }

    struct Block {
        header: Option<String>,
        lines: Vec<String>,
    }
    let mut blocks: Vec<Block> = Vec::new();
    for line in contents.lines() {
        let t = line.trim();
        let is_header = (t.starts_with("[[") && t.ends_with("]]"))
            || (t.starts_with('[') && t.ends_with(']') && !t.contains('='));
        if is_header {
            blocks.push(Block { header: Some(t.to_string()), lines: vec![line.to_string()] });
        } else if let Some(last) = blocks.last_mut() {
            last.lines.push(line.to_string());
        } else {
            blocks.push(Block { header: None, lines: vec![line.to_string()] });
        }
    }

    // Count simple (single-segment, non-array) table headers.
    let mut counts: HashMap<String, usize> = HashMap::new();
    for b in &blocks {
        if let Some(h) = &b.header {
            if !h.starts_with("[[") && !h.contains(" \"") {
                *counts.entry(h.clone()).or_default() += 1;
            }
        }
    }

    fn key_of(line: &str) -> Option<String> {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            return None;
        }
        let eq = t.find('=')?;
        let k = t[..eq].trim().to_string();
        if k.is_empty() || k.contains(char::is_whitespace) {
            None
        } else {
            Some(k)
        }
    }

    let mut seen_headers: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<String> = Vec::new();
    for b in &blocks {
        let dup = b
            .header
            .as_ref()
            .map(|h| counts.get(h).copied().unwrap_or(0) > 1)
            .unwrap_or(false);
        if !dup {
            out.extend(b.lines.iter().cloned());
            continue;
        }
        let h = b.header.clone().unwrap();
        if seen_headers.contains_key(&h) {
            continue; // duplicate block: folded into the first occurrence
        }
        seen_headers.insert(h.clone(), 1);
        out.push(b.lines[0].clone());
        // Merge bodies of ALL occurrences in file order, first key wins.
        let mut keys: HashSet<String> = HashSet::new();
        let mut body: Vec<String> = Vec::new();
        let mut comment_lines: Vec<String> = Vec::new();
        for b2 in blocks.iter().filter(|x| x.header.as_ref() == Some(&h)) {
            for line in b2.lines.iter().skip(1) {
                match key_of(line) {
                    Some(k) => {
                        if keys.insert(k) {
                            body.push(line.clone());
                        }
                    }
                    None => {
                        if !line.trim().is_empty() {
                            comment_lines.push(line.clone());
                        }
                    }
                }
            }
        }
        out.extend(body);
        out.extend(comment_lines);
    }
    out.join("\n")
}
pub fn save_config(config: &Config, path: &str) -> Result<(), ConfigError> {
    let contents = toml::to_string_pretty(config).map_err(|e| {
        ConfigError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    })?;
    // Atomic write: temp file + rename, so concurrent readers never see a
    // half-written config.
    let tmp_path = format!("{}.tmp", path);
    std::fs::write(&tmp_path, contents).map_err(ConfigError::Io)?;
    std::fs::rename(&tmp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        ConfigError::Io(e)
    })?;
    Ok(())
}

/// Load both registry and user config, then merge.
pub fn load_config_with_registry(config_path: &str, registry_path: &str) -> Result<Config, ConfigError> {
    let registry = load_registry(registry_path)?;
    let mut config = load_config(config_path)?;
    config.merge_registry(&registry);
    Ok(config)
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(toml::de::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model_id_with_override() {
        let ref_ = ModelProviderRef {
            provider: "anthropic".to_string(),
            model_id: Some("claude-3-5-sonnet-20241022".to_string()),
        };
        assert_eq!(
            resolve_model_id_standalone("claude-3-5-sonnet", "anthropic", &ref_),
            "claude-3-5-sonnet-20241022"
        );
    }

    #[test]
    fn test_resolve_model_id_without_override() {
        let ref_ = ModelProviderRef {
            provider: "anthropic".to_string(),
            model_id: None,
        };
        assert_eq!(
            resolve_model_id_standalone("claude-3-5-sonnet", "anthropic", &ref_),
            "claude-3-5-sonnet"
        );
    }

    #[test]
    fn test_merge_registry_fills_base_url() {
        let mut config = Config::default();
        config.providers.insert("groq".to_string(), ProviderConfig {
            api_key: "gsk-test".to_string(),
            base_url: String::new(),
            connect_timeout_ms: 8000,
            read_timeout_ms: 8000,
            ttft_timeout_ms: 60000,
            generation_timeout_ms: 90000,
            total_timeout_ms: 240000,
            priority: None,
            provider_type: None,
            capabilities: vec![],
            enabled: true,
        });

        let registry: ProviderRegistry = HashMap::from([(
            "groq".to_string(),
            ProviderRegistryEntry {
                base_url: "https://api.groq.com/openai".to_string(),
                auth_type: "bearer".to_string(),
                models_path: "/v1/models".to_string(),
                label: "Groq".to_string(),
            },
        )]);

        config.merge_registry(&registry);
        assert_eq!(config.providers["groq"].base_url, "https://api.groq.com/openai");
    }

    #[test]
    fn test_load_registry_from_file() {
        // The bundled providers.toml should have a reasonable number of entries
        let registry = load_registry("config/providers.toml");
        assert!(registry.is_ok(), "Failed to load providers.toml");
        let registry = registry.unwrap();
        assert!(registry.len() > 50, "Expected >50 providers, got {}", registry.len());
        assert!(registry.contains_key("openai"));
        assert!(registry.contains_key("anthropic"));
        assert!(registry.contains_key("google"));
        assert!(registry.contains_key("deepseek"));
        assert!(registry.contains_key("groq"));
    }

    #[test]
    fn test_mask_key() {
        assert_eq!(mask_key("sk-abcdefghijklmnop1234"), "sk-abc***1234");
        assert_eq!(mask_key("short-key"), "***");
        assert_eq!(mask_key(""), "***");
    }

    #[test]
    fn test_enabled_defaults_true_when_missing() {
        let toml_src = r#"
[providers.demo]
api_key = "k"
base_url = "https://x"
"#;
        let config: Config = toml::from_str(toml_src).unwrap();
        assert!(config.providers["demo"].enabled);
    }

    #[test]
    fn test_dedup_merges_duplicate_timeouts() {
        // A legacy hand-written [timeouts] plus a console-written one used to
        // panic every fresh boot ("invalid TOML"). Merge must keep the FIRST
        // assignment of each key and drop the rest.
        let raw = r#"
[timeouts]
sse_inactivity_secs = 1
deadline_secs = 2

[timeouts]
sse_inactivity_secs = 120
deadline_secs = 300
model_header_timeout_secs = 15

[providers.demo]
api_key = "k"
base_url = "https://x"
"#;
        let fixed = dedup_toml_tables(raw);
        let config: Config = toml::from_str(&fixed).expect("merged config should parse");
        assert_eq!(config.timeouts.sse_inactivity_secs, 1);
        assert_eq!(config.timeouts.deadline_secs, 2);
        assert!(config.providers.contains_key("demo"));
    }

    #[test]
    fn test_dedup_passes_healthy_config_through() {
        let raw = r#"
[timeouts]
sse_inactivity_secs = 120

[providers.demo]
api_key = "k"
base_url = "https://x"

[[aot]]
name = "x"
"#;
        assert_eq!(dedup_toml_tables(raw), raw.to_string());
    }

    #[test]
    fn test_dedup_still_rejects_genuinely_broken_toml() {
        // Missing bracket must NOT be silently "repaired" — parse must fail.
        let raw = "[providers\napi_key = \"k\"\n";
        let fixed = dedup_toml_tables(raw);
        assert!(toml::from_str::<Config>(&fixed).is_err());
    }

    // Standalone helper for tests (no Config needed).
    fn resolve_model_id_standalone(
        group_name: &str,
        _provider_name: &str,
        mpr: &ModelProviderRef,
    ) -> String {
        mpr.model_id.clone().unwrap_or_else(|| group_name.to_string())
    }
}
