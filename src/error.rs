//! Error classification ported from FreeLLMAPI's `error-classify.ts`.
//!
//! Every error triggers immediate failover — no cooldowns, no retries
//! to the same provider. The classification tells the router *how* to
//! skip (skip this provider, skip this model group, etc.).

use thiserror::Error;

/// Classification of a provider error / non-2xx response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorClass {
    /// Transient errors: 429, 502, 503, 504, network errors, timeouts, 500.
    /// Failover to the next provider immediately.
    Retryable,
    /// The model was not found at this provider.
    /// Skip this provider entirely for this model going forward.
    ModelNotFound,
    /// Access forbidden (403 / "permission denied").
    /// Skip this provider for this request.
    ModelAccessForbidden,
    /// Payment required (402, insufficient balance).
    /// Skip this provider for this request.
    PaymentRequired,
    /// Bad request from the provider — validation/malformed (400).
    /// Skip this provider for this request.
    ProviderBadRequest,
}

impl ErrorClass {
    /// Whether this error should trigger an immediate failover.
    /// Every error class triggers failover — there are no cooldowns.
    pub fn should_fallback(&self) -> bool {
        true
    }
}

#[derive(Error, Debug)]
pub struct ProviderError {
    pub class: ErrorClass,
    pub status: Option<u16>,
    pub body: String,
    pub retry_after_ms: Option<u64>,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let retry = match self.retry_after_ms {
            Some(ms) => format!("{}", ms),
            None => "none".to_string(),
        };
        write!(
            f,
            "ProviderError({:?}, status={:?}, retry_after_ms={})",
            self.class, self.status, retry
        )
    }
}

/// The retryable HTTP status codes (ported from FreeLLMAPI).
pub const RETRYABLE_STATUS_CODES: &[u16] = &[429, 500, 502, 503, 504];

/// Classify a provider response by status code and body text.
/// This is the core port of FreeLLMAPI's `isRetryableError`,
/// `isModelNotFoundError`, `isModelAccessForbiddenError`, etc.
pub fn classify_error(status: u16, body: &str) -> ErrorClass {
    // --- Model not found ---
    if is_model_not_found_error(status, body) {
        return ErrorClass::ModelNotFound;
    }

    // --- Access forbidden ---
    if is_model_access_forbidden_error(status, body) {
        return ErrorClass::ModelAccessForbidden;
    }

    // --- Payment required ---
    if is_payment_required_error(status, body) {
        return ErrorClass::PaymentRequired;
    }

    // --- Provider bad request ---
    if is_provider_bad_request_error(status, body) {
        return ErrorClass::ProviderBadRequest;
    }

    // --- Retryable (default for transient) ---
    ErrorClass::Retryable
}

/// 429, 502, 503, 504, 500, or network/timeout errors.
pub fn is_retryable_error(status: u16, body: &str) -> bool {
    RETRYABLE_STATUS_CODES.contains(&status) || body.to_lowercase().contains("timeout") || body.to_lowercase().contains("network")
}

/// Model not found: 404, or message contains "model not found" / "model doesn't exist".
pub fn is_model_not_found_error(status: u16, body: &str) -> bool {
    if status == 404 {
        return true;
    }
    let lower = body.to_lowercase();
    lower.contains("model not found")
        || lower.contains("model doesn't exist")
        || lower.contains("no such model")
        || lower.contains("model not available")
}

/// Access forbidden: 403, or message contains "access forbidden" / "permission denied".
pub fn is_model_access_forbidden_error(status: u16, body: &str) -> bool {
    if status == 403 {
        return true;
    }
    let lower = body.to_lowercase();
    lower.contains("access forbidden")
        || lower.contains("permission denied")
        || lower.contains("not authorized")
        || lower.contains("unauthorized")
}

/// Payment required: 402, or message contains "insufficient balance" / "quota exceeded".
pub fn is_payment_required_error(status: u16, body: &str) -> bool {
    if status == 402 {
        return true;
    }
    let lower = body.to_lowercase();
    lower.contains("insufficient balance")
        || lower.contains("quota exceeded")
        || lower.contains("payment required")
        || lower.contains("billing")
        || lower.contains("credit")
}

/// Bad request: 400, or message contains "bad request" / "invalid" / "validation".
/// Note: this is checked *after* model-not-found and forbidden, since 404/403
/// are more specific. Some providers return 400 for model-not-found.
pub fn is_provider_bad_request_error(status: u16, body: &str) -> bool {
    if status == 400 {
        return true;
    }
    let lower = body.to_lowercase();
    lower.contains("bad request")
        || lower.contains("invalid")
        || lower.contains("validation")
        || lower.contains("malformed")
}

/// Parse `retry-after` from a response body or header.
/// Ported from FreeLLMAPI's `parseRetryAfterMs`.
/// Handles:
/// - Header value: "120" (seconds) or HTTP-date
/// - Body JSON: `{"error": {"param": "retry-after", "message": "120"}}` or
///   `{"retry_after": 120}` or `{"retry-after": "120"}`
pub fn parse_retry_after_ms(status: u16, body: &str, retry_after_header: Option<&str>) -> Option<u64> {
    // 1. Try header first (most reliable).
    if let Some(header) = retry_after_header {
        if let Some(ms) = parse_retry_after_header(header) {
            return Some(ms);
        }
    }

    // 2. Try body JSON.
    if let Some(ms) = parse_retry_after_body(body) {
        return Some(ms);
    }

    // 3. Default retry-after for known retryable statuses.
    if RETRYABLE_STATUS_CODES.contains(&status) {
        return Some(1000); // 1 second default
    }

    None
}

fn parse_retry_after_header(value: &str) -> Option<u64> {
    let value = value.trim();

    // Numeric — could be seconds or an integer.
    if let Ok(secs) = value.parse::<u64>() {
        // HTTP spec says Retry-After is in seconds for HTTP-date or delta-seconds.
        return Some(secs * 1000);
    }

    // HTTP-date format — parse and compute delta. (simplified)
    // For now, fall through to default behavior.
    None
}

fn parse_retry_after_body(body: &str) -> Option<u64> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;

    // Google-style
    if let Some(v) = parsed.get("retry_after") {
        return v.as_u64().map(|s| s * 1000).or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()).map(|s| s * 1000));
    }

    // OpenAI-style: {"error": {"param": "retry-after", ...}}  — not standard, but handle
    if let Some(err) = parsed.get("error") {
        if let Some(param) = err.get("param").and_then(|p| p.as_str()) {
            if param == "retry-after" {
                if let Some(msg) = err.get("message").and_then(|m| m.as_str()) {
                    if let Ok(s) = msg.parse::<u64>() {
                        return Some(s * 1000);
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable_error() {
        assert!(is_retryable_error(429, "rate limited"));
        assert!(is_retryable_error(500, "internal"));
        assert!(is_retryable_error(502, "bad gateway"));
        assert!(is_retryable_error(503, "unavailable"));
        assert!(is_retryable_error(504, "gateway timeout"));
        assert!(is_retryable_error(200, "connection timeout"));
        assert!(!is_retryable_error(400, "bad request"));
        assert!(!is_retryable_error(404, "not found"));
    }

    #[test]
    fn test_is_model_not_found_error() {
        assert!(is_model_not_found_error(404, "anything"));
        assert!(is_model_not_found_error(200, "model not found"));
        assert!(is_model_not_found_error(200, "Model doesn't exist"));
        assert!(is_model_not_found_error(200, "no such model"));
        assert!(is_model_not_found_error(200, "model not available"));
        assert!(!is_model_not_found_error(400, "bad request"));
    }

    #[test]
    fn test_is_model_access_forbidden_error() {
        assert!(is_model_access_forbidden_error(403, "anything"));
        assert!(is_model_access_forbidden_error(200, "access forbidden"));
        assert!(is_model_access_forbidden_error(200, "permission denied"));
        assert!(is_model_access_forbidden_error(200, "not authorized"));
        assert!(is_model_access_forbidden_error(200, "unauthorized"));
        assert!(!is_model_access_forbidden_error(400, "bad request"));
    }

    #[test]
    fn test_is_payment_required_error() {
        assert!(is_payment_required_error(402, "anything"));
        assert!(is_payment_required_error(200, "insufficient balance"));
        assert!(is_payment_required_error(200, "quota exceeded"));
        assert!(is_payment_required_error(200, "payment required"));
        assert!(is_payment_required_error(200, "billing issue"));
        assert!(is_payment_required_error(200, "credit low"));
        assert!(!is_payment_required_error(400, "bad request"));
    }

    #[test]
    fn test_is_provider_bad_request_error() {
        assert!(is_provider_bad_request_error(400, "anything"));
        assert!(is_provider_bad_request_error(200, "bad request"));
        assert!(is_provider_bad_request_error(200, "invalid parameter"));
        assert!(is_provider_bad_request_error(200, "validation error"));
        assert!(is_provider_bad_request_error(200, "malformed request"));
    }

    #[test]
    fn test_classify_error() {
        assert_eq!(classify_error(404, "not found"), ErrorClass::ModelNotFound);
        assert_eq!(classify_error(403, "forbidden"), ErrorClass::ModelAccessForbidden);
        assert_eq!(classify_error(402, "no money"), ErrorClass::PaymentRequired);
        assert_eq!(classify_error(400, "bad"), ErrorClass::ProviderBadRequest);
        assert_eq!(classify_error(500, "server"), ErrorClass::Retryable);
        assert_eq!(classify_error(503, "busy"), ErrorClass::Retryable);
    }

    #[test]
    fn test_parse_retry_after_header_seconds() {
        assert_eq!(parse_retry_after_ms(429, "", Some("120")), Some(120_000));
    }

    #[test]
    fn test_parse_retry_after_body_google_style() {
        let body = r#"{"retry_after": 2}"#;
        assert_eq!(parse_retry_after_ms(429, body, None), Some(2_000));
    }

    #[test]
    fn test_parse_retry_after_default_for_retryable() {
        assert_eq!(parse_retry_after_ms(503, "{}", None), Some(1000));
    }
}
