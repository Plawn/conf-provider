use metrics::{counter, describe_counter, describe_histogram, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::time::Instant;

/// Initialize the Prometheus metrics recorder and return the handle for scraping.
pub fn init_metrics() -> PrometheusHandle {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // Describe metrics
    describe_counter!(
        "config_reloads_total",
        "Total number of configuration reload operations"
    );
    describe_counter!(
        "config_renders_total",
        "Total number of configuration render operations"
    );
    describe_histogram!(
        "config_render_duration_seconds",
        "Configuration render duration in seconds"
    );
    describe_counter!(
        "git_cache_lookups_total",
        "Total number of git DAG cache lookups"
    );

    // Initialize counters with zero so they appear in output immediately
    // We use a placeholder label that won't conflict with real labels
    counter!("config_reloads_total", "success" => "true").absolute(0);
    counter!("config_reloads_total", "success" => "false").absolute(0);
    counter!("git_cache_lookups_total", "hit" => "true").absolute(0);
    counter!("git_cache_lookups_total", "hit" => "false").absolute(0);

    handle
}

/// Record an HTTP request with its method, path, status, and duration.
pub fn record_request(method: &str, path: &str, status: u16, duration: std::time::Duration) {
    let labels = [
        ("method", method.to_string()),
        ("path", path.to_string()),
        ("status", status.to_string()),
    ];

    counter!("http_requests_total", &labels).increment(1);
    histogram!("http_request_duration_seconds", &labels).record(duration.as_secs_f64());
}

/// Record a config reload event.
pub fn record_reload(success: bool) {
    let labels = [("success", success.to_string())];
    counter!("config_reloads_total", &labels).increment(1);
}

/// Record a config render operation.
pub fn record_render(format: &str, success: bool, duration: std::time::Duration) {
    let labels = [
        ("format", format.to_string()),
        ("success", success.to_string()),
    ];

    counter!("config_renders_total", &labels).increment(1);
    histogram!("config_render_duration_seconds", &labels).record(duration.as_secs_f64());
}

/// Record a git cache hit or miss.
pub fn record_git_cache(hit: bool) {
    let labels = [("hit", hit.to_string())];
    counter!("git_cache_lookups_total", &labels).increment(1);
}

/// A guard that records request duration when dropped.
pub struct RequestTimer {
    start: Instant,
    method: String,
    path: String,
}

impl RequestTimer {
    pub fn new(method: &str, path: &str) -> Self {
        Self {
            start: Instant::now(),
            method: method.to_string(),
            path: path.to_string(),
        }
    }

    pub fn finish(self, status: u16) {
        record_request(&self.method, &self.path, status, self.start.elapsed());
    }
}
