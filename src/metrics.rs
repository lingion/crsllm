use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Metrics for a single provider
#[derive(Debug, Clone)]
pub struct ProviderMetrics {
    pub provider_name: String,
    pub ttft_samples: Vec<Duration>,
    pub total_time_samples: Vec<Duration>,
    pub generation_speed_samples: Vec<f64>, // tokens per second
    pub timeout_count: u32,
    pub error_429_count: u32,
    pub success_count: u32,
    pub total_requests: u32,
    pub last_used: Option<Instant>,
}

impl ProviderMetrics {
    pub fn new(provider_name: String) -> Self {
        Self {
            provider_name,
            ttft_samples: Vec::new(),
            total_time_samples: Vec::new(),
            generation_speed_samples: Vec::new(),
            timeout_count: 0,
            error_429_count: 0,
            success_count: 0,
            total_requests: 0,
            last_used: None,
        }
    }

    pub fn record_success(&mut self, ttft: Duration, total_time: Duration, output_tokens: u32) {
        self.ttft_samples.push(ttft);
        self.total_time_samples.push(total_time);
        
        // Calculate generation speed (tokens per second)
        if output_tokens > 0 {
            let total_secs = total_time.as_secs_f64();
            if total_secs > 0.0 {
                let speed = output_tokens as f64 / total_secs;
                self.generation_speed_samples.push(speed);
            }
        }
        
        self.success_count += 1;
        self.total_requests += 1;
        self.last_used = Some(Instant::now());

        // Keep only last 100 samples
        if self.ttft_samples.len() > 100 {
            self.ttft_samples.remove(0);
        }
        if self.total_time_samples.len() > 100 {
            self.total_time_samples.remove(0);
        }
        if self.generation_speed_samples.len() > 100 {
            self.generation_speed_samples.remove(0);
        }
    }

    pub fn record_timeout(&mut self) {
        self.timeout_count += 1;
        self.total_requests += 1;
        self.last_used = Some(Instant::now());
    }

    pub fn record_429(&mut self) {
        self.error_429_count += 1;
        self.total_requests += 1;
        self.last_used = Some(Instant::now());
    }

    pub fn record_error(&mut self) {
        self.total_requests += 1;
        self.last_used = Some(Instant::now());
    }

    pub fn median_ttft(&self) -> Option<Duration> {
        if self.ttft_samples.is_empty() {
            return None;
        }
        let mut sorted = self.ttft_samples.clone();
        sorted.sort();
        Some(sorted[sorted.len() / 2])
    }

    pub fn p95_ttft(&self) -> Option<Duration> {
        if self.ttft_samples.is_empty() {
            return None;
        }
        let mut sorted = self.ttft_samples.clone();
        sorted.sort();
        let idx = (sorted.len() as f64 * 0.95) as usize;
        Some(sorted[idx.min(sorted.len() - 1)])
    }

    pub fn median_total_time(&self) -> Option<Duration> {
        if self.total_time_samples.is_empty() {
            return None;
        }
        let mut sorted = self.total_time_samples.clone();
        sorted.sort();
        Some(sorted[sorted.len() / 2])
    }

    pub fn median_generation_speed(&self) -> Option<f64> {
        if self.generation_speed_samples.is_empty() {
            return None;
        }
        let mut sorted = self.generation_speed_samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Some(sorted[sorted.len() / 2])
    }

    pub fn timeout_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        self.timeout_count as f64 / self.total_requests as f64
    }

    pub fn error_429_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        self.error_429_count as f64 / self.total_requests as f64
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        self.success_count as f64 / self.total_requests as f64
    }
}

/// One completed request record (result of a full route attempt chain).
#[derive(Debug, Clone, serde::Serialize)]
pub struct RequestLogEntry {
    /// Unix ms when the request finished.
    pub ts_ms: u64,
    /// Logical model / group the client asked for.
    pub model: String,
    /// Providers attempted, in order.
    pub chain: Vec<String>,
    /// "ok" | "error" | "timeout" | "429" | "exhausted"
    pub result: String,
    /// Error class text when not ok.
    pub error: Option<String>,
    pub ttft_ms: Option<u64>,
    pub total_ms: u64,
    pub output_tokens: Option<u64>,
    /// True when the request went through the SSE streaming path.
    pub streaming: bool,
}

/// A single sampled event for time-series display.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryPoint {
    pub ts_ms: u64,
    /// 1 = success, 0 = failure of any kind.
    pub ok: u8,
    pub ttft_ms: Option<u64>,
}

/// Global metrics store for all providers
#[derive(Debug, Clone)]
pub struct MetricsStore {
    metrics: Arc<RwLock<HashMap<String, ProviderMetrics>>>,
    logs: Arc<RwLock<std::collections::VecDeque<RequestLogEntry>>>,
    history: Arc<RwLock<HashMap<String, std::collections::VecDeque<HistoryPoint>>>>,
    started_at: Instant,
}

impl MetricsStore {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            logs: Arc::new(RwLock::new(std::collections::VecDeque::new())),
            history: Arc::new(RwLock::new(HashMap::new())),
            started_at: Instant::now(),
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Append a request log entry, keeping at most 1000 (ring buffer).
    pub fn log_request(&self, entry: RequestLogEntry) {
        let mut logs = self.logs.write().unwrap();
        logs.push_back(entry);
        while logs.len() > 1000 {
            logs.pop_front();
        }
    }

    /// Most recent `limit` log entries, newest first.
    pub fn recent_logs(&self, limit: usize) -> Vec<RequestLogEntry> {
        let logs = self.logs.read().unwrap();
        logs.iter().rev().take(limit).cloned().collect()
    }

    fn push_history(&self, provider_name: &str, ok: bool, ttft_ms: Option<u64>) {
        let mut history = self.history.write().unwrap();
        let q = history
            .entry(provider_name.to_string())
            .or_insert_with(std::collections::VecDeque::new);
        q.push_back(HistoryPoint {
            ts_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            ok: if ok { 1 } else { 0 },
            ttft_ms,
        });
        while q.len() > 240 {
            q.pop_front();
        }
    }

    pub fn get_history(&self) -> HashMap<String, Vec<HistoryPoint>> {
        let history = self.history.read().unwrap();
        history.iter().map(|(k, v)| (k.clone(), v.iter().cloned().collect())).collect()
    }

    pub fn record_success(&self, provider_name: &str, ttft: Duration, total_time: Duration, output_tokens: u32) {
        {
            let mut metrics = self.metrics.write().unwrap();
            let provider_metrics = metrics
                .entry(provider_name.to_string())
                .or_insert_with(|| ProviderMetrics::new(provider_name.to_string()));
            provider_metrics.record_success(ttft, total_time, output_tokens);
        }
        self.push_history(provider_name, true, Some(ttft.as_millis() as u64));
    }

    pub fn record_timeout(&self, provider_name: &str) {
        {
            let mut metrics = self.metrics.write().unwrap();
            let provider_metrics = metrics
                .entry(provider_name.to_string())
                .or_insert_with(|| ProviderMetrics::new(provider_name.to_string()));
            provider_metrics.record_timeout();
        }
        self.push_history(provider_name, false, None);
    }

    pub fn record_429(&self, provider_name: &str) {
        {
            let mut metrics = self.metrics.write().unwrap();
            let provider_metrics = metrics
                .entry(provider_name.to_string())
                .or_insert_with(|| ProviderMetrics::new(provider_name.to_string()));
            provider_metrics.record_429();
        }
        self.push_history(provider_name, false, None);
    }

    /// Record a generic provider error (non-429/non-timeout) so the request
    /// still counts toward success-rate and history time series.
    pub fn record_error(&self, provider_name: &str) {
        {
            let mut metrics = self.metrics.write().unwrap();
            let provider_metrics = metrics
                .entry(provider_name.to_string())
                .or_insert_with(|| ProviderMetrics::new(provider_name.to_string()));
            provider_metrics.record_error();
        }
        self.push_history(provider_name, false, None);
    }

    pub fn get_metrics(&self, provider_name: &str) -> Option<ProviderMetrics> {
        let metrics = self.metrics.read().unwrap();
        metrics.get(provider_name).cloned()
    }

    pub fn get_all_metrics(&self) -> HashMap<String, ProviderMetrics> {
        let metrics = self.metrics.read().unwrap();
        metrics.clone()
    }

    /// Calculate adaptive priority score for a provider
    /// Lower score = better priority
    /// Score based on: median TTFT, timeout rate, 429 rate
    pub fn calculate_priority_score(&self, provider_name: &str) -> Option<f64> {
        let metrics = self.get_metrics(provider_name)?;
        
        if metrics.total_requests < 3 {
            // Not enough data, return neutral score
            return Some(50.0);
        }

        let median_ttft = metrics.median_ttft()?;
        let ttft_score = median_ttft.as_millis() as f64 / 1000.0; // Normalize to seconds
        
        let timeout_penalty = metrics.timeout_rate() * 100.0;
        let error_429_penalty = metrics.error_429_rate() * 50.0;
        
        Some(ttft_score + timeout_penalty + error_429_penalty)
    }

    /// Sort providers by adaptive priority
    pub fn sort_providers_by_priority(&self, providers: Vec<String>) -> Vec<String> {
        let mut scored: Vec<(String, f64)> = providers
            .into_iter()
            .map(|p| {
                let score = self.calculate_priority_score(&p).unwrap_or(100.0);
                (p, score)
            })
            .collect();
        
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        scored.into_iter().map(|(p, _)| p).collect()
    }
}

impl Default for MetricsStore {
    fn default() -> Self {
        Self::new()
    }
}
