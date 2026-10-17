//! SSE streaming with failover.
//!
//! - Forward SSE events from provider as received.
//! - First-token timeout (per provider `ttft_timeout_ms`, relaxed to >=60s for
//!   free providers) and inter-token inactivity timeout (`generation_timeout_ms`)
//!   → failover to next provider.
//! - Abrupt EOF (stream closes without data) → failover to next provider.
//! - Honors routed model chains + adaptive provider priority (same policy as
//!   `Router::route`).
//! - Records per-request latency (TTFT, total) and generation speed into the
//!   shared metrics store.

use async_stream::stream;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::{ErrorClass, ProviderError};
use crate::router::Router;
use crate::translator::AnthropicRequest;

/// A stream of SSE chunks, each a string of SSE-formatted text.
pub type SseChunk = Result<String, ProviderError>;

/// Stream a request through the fallback chain.
pub fn stream_with_fallback(
    router: Arc<Router>,
    model_group_name: String,
    req: AnthropicRequest,
) -> Pin<Box<dyn Stream<Item = SseChunk> + Send>> {
    Box::pin(stream! {
        let chain = router.build_fallback_chain(&model_group_name).await;

        let request_start = Instant::now();
        let mut attempted: Vec<String> = Vec::new();

        'chain: for group_name in &chain {
            let config = router.config_snapshot().await;

            // Manual model group first, then auto-generated groups from
            // discovered models (same policy as Router::route).
            let group = if let Some(g) = config.model_group(group_name) {
                g.clone()
            } else {
                let auto_groups =
                    crate::handler::auto_generate_model_groups(&config.discovered);
                match auto_groups.get(group_name) {
                    Some(refs) => crate::config::ModelGroup {
                        providers: refs.clone(),
                    },
                    None => continue 'chain,
                }
            };

            // Adaptive provider ordering within the group.
            let provider_refs = if router.metrics().get_all_metrics().is_empty() {
                group.providers.clone()
            } else {
                let names: Vec<String> =
                    group.providers.iter().map(|p| p.provider.clone()).collect();
                let sorted = router.metrics().sort_providers_by_priority(names);
                sorted.iter()
                    .filter_map(|n| group.providers.iter().find(|p| &p.provider == n).cloned())
                    .collect()
            };
            drop(config);

            for provider_ref in &provider_refs {
                let config = router.config_snapshot().await;
                let provider_config = match config.provider(&provider_ref.provider) {
                    Some(p) => p.clone(),
                    None => continue,
                };
                // Disabled providers are skipped entirely.
                if !provider_config.enabled {
                    continue;
                }
                attempted.push(provider_ref.provider.clone());
                let model_id = config.resolve_model_id(
                    group_name,
                    &provider_ref.provider,
                    provider_ref,
                );
                drop(config);

                let adapter = router.create_adapter(
                    &provider_ref.provider,
                    provider_config.clone(),
                    model_id.clone(),
                );

                // Free/public providers: first-token window at least 60s
                // (they are slow to warm up but genuinely free).
                let first_token_timeout = if provider_config.provider_type.as_deref() == Some("free") {
                    Duration::from_millis(provider_config.ttft_timeout_ms.max(60_000))
                } else {
                    Duration::from_millis(provider_config.ttft_timeout_ms)
                };
                let inactivity_timeout =
                    Duration::from_millis(provider_config.generation_timeout_ms.max(1_000));

                let attempt_start = Instant::now();
                let resp = match adapter.stream(&req, &model_id).await {
                    Ok(r) => r,
                    Err(e) => {
                        router.metrics().record_timeout(&provider_ref.provider);
                        tracing::warn!(
                            "Provider {} stream request failed after {:?}: {}",
                            provider_ref.provider,
                            attempt_start.elapsed(),
                            e
                        );
                        continue;
                    }
                };

                let status = resp.status().as_u16();
                if status >= 400 {
                    let body = resp.text().await.unwrap_or_default();
                    let err = adapter.make_error_from_status(status, &body);
                    if status == 429 {
                        router.metrics().record_429(&provider_ref.provider);
                        tracing::warn!(
                            "Provider {} stream rate-limited (429): {}",
                            provider_ref.provider,
                            body
                        );
                        continue;
                    }
                    tracing::warn!(
                        "Provider {} stream returned error {}: {}",
                        provider_ref.provider,
                        status,
                        body
                    );
                    match err.class {
                        ErrorClass::ModelNotFound
                        | ErrorClass::ModelAccessForbidden
                        | ErrorClass::PaymentRequired => {
                            // Fatal for this group: skip to next group in the chain.
                            continue 'chain;
                        }
                        _ => continue,
                    }
                }

                let byte_stream = resp.bytes_stream();
                tokio::pin!(byte_stream);

                let mut buffer: Vec<u8> = Vec::new();
                let mut got_data = false;
                let mut ttft: Option<Duration> = None;
                let mut delta_events: u64 = 0;
                let mut timed_out = false;

                loop {
                    let budget = if ttft.is_none() {
                        first_token_timeout
                    } else {
                        inactivity_timeout
                    };
                    let timeout_result = tokio::time::timeout(budget, byte_stream.next()).await;

                    match timeout_result {
                        Ok(Some(Ok(chunk))) => {
                            buffer.extend_from_slice(&chunk);

                            while let Some(event_end) = find_sse_event_end(&buffer) {
                                let event =
                                    buffer.drain(..=event_end).collect::<Vec<_>>();
                                let event_str =
                                    String::from_utf8_lossy(&event).to_string();
                                if !event_str.trim().is_empty() {
                                    if ttft.is_none() {
                                        ttft = Some(attempt_start.elapsed());
                                    }
                                    if is_delta_event(&event_str) {
                                        delta_events += 1;
                                    }
                                    got_data = true;
                                    yield Ok(event_str);
                                }
                            }
                        }
                        Ok(Some(Err(_))) => {
                            tracing::warn!(
                                "Provider {} stream read error, failing over",
                                provider_ref.provider
                            );
                            break;
                        }
                        Ok(None) => {
                            if !buffer.is_empty() {
                                let event_str =
                                    String::from_utf8_lossy(&buffer).to_string();
                                if !event_str.trim().is_empty() {
                                    if ttft.is_none() {
                                        ttft = Some(attempt_start.elapsed());
                                    }
                                    if is_delta_event(&event_str) {
                                        delta_events += 1;
                                    }
                                    got_data = true;
                                    yield Ok(event_str);
                                }
                            }
                            if !got_data {
                                tracing::warn!(
                                    "Provider {} stream ended without data, failing over",
                                    provider_ref.provider
                                );
                                router.metrics().record_timeout(&provider_ref.provider);
                                break;
                            }
                            // Success: record this request's latency + speed.
                            let total = attempt_start.elapsed();
                            let ttft_val = ttft.unwrap_or(total);
                            router.metrics().record_success(
                                &provider_ref.provider,
                                ttft_val,
                                total,
                                delta_events as u32,
                            );
                            router.metrics().log_request(crate::metrics::RequestLogEntry {
                                ts_ms: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or(0),
                                model: model_group_name.clone(),
                                chain: attempted.clone(),
                                result: "ok".to_string(),
                                error: None,
                                ttft_ms: Some(ttft_val.as_millis() as u64),
                                total_ms: request_start.elapsed().as_millis() as u64,
                                output_tokens: Some(delta_events),
                                streaming: true,
                            });
                            return;
                        }
                        Err(_) => {
                            tracing::warn!(
                                "Provider {} SSE inactivity timeout ({:?}), failing over",
                                provider_ref.provider,
                                if ttft.is_none() { first_token_timeout } else { inactivity_timeout }
                            );
                            timed_out = true;
                            break;
                        }
                    }
                }

                if timed_out {
                    router.metrics().record_timeout(&provider_ref.provider);
                }
            }
        }

        router.metrics().log_request(crate::metrics::RequestLogEntry {
            ts_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            model: model_group_name.clone(),
            chain: attempted.clone(),
            result: "exhausted".to_string(),
            error: Some("all streaming providers failed".to_string()),
            ttft_ms: None,
            total_ms: request_start.elapsed().as_millis() as u64,
            output_tokens: None,
            streaming: true,
        });
        yield Err(ProviderError {
            class: ErrorClass::Retryable,
            status: None,
            body: "All streaming providers failed".to_string(),
            retry_after_ms: None,
        });
    })
}

/// Heuristic: does this SSE event carry generated content (counted as a
/// "token" for generation-speed estimation)? Anthropic text deltas and
/// OpenAI chat deltas both qualify.
fn is_delta_event(event: &str) -> bool {
    event.contains("content_block_delta")
        || (event.contains("\"delta\"") && !event.contains("\"delta\":{}"))
}

/// Find the end index of a complete SSE event in the buffer.
/// SSE events end with a blank line (\n\n).
fn find_sse_event_end(buffer: &[u8]) -> Option<usize> {
    for i in 0..buffer.len() {
        if buffer[i] == b'\n' {
            if i + 1 < buffer.len() && buffer[i + 1] == b'\n' {
                return Some(i + 1);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_sse_event_end() {
        let buf = b"data: hello\n\ndata: world\n\n";
        let pos = find_sse_event_end(buf).unwrap();
        assert_eq!(&buf[..=pos], b"data: hello\n\n");
    }

    #[test]
    fn test_find_sse_event_end_no_event() {
        let buf = b"data: incomple";
        assert!(find_sse_event_end(buf).is_none());
    }

    #[test]
    fn test_is_delta_event() {
        assert!(is_delta_event("event: content_block_delta\ndata: {\"type\":\"content_block_delta\"}\n\n"));
        assert!(is_delta_event("data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n"));
        assert!(!is_delta_event("event: message_start\ndata: {\"type\":\"message_start\"}\n\n"));
        assert!(!is_delta_event("data: [DONE]\n\n"));
    }
}
