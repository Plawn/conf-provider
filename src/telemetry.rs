use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, trace::TracerProvider};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Configuration for telemetry/tracing.
pub struct TelemetryConfig {
    /// OTLP endpoint for sending traces (e.g., "http://localhost:4317")
    pub otlp_endpoint: Option<String>,
    /// Service name for tracing
    pub service_name: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            service_name: "konf-provider".to_string(),
        }
    }
}

/// Initialize the tracing subscriber with optional OpenTelemetry export.
///
/// Returns the tracer provider if OpenTelemetry was configured (for graceful shutdown).
pub fn init_tracing(config: TelemetryConfig) -> Option<TracerProvider> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        format!(
            "{}=debug,tower_http=debug,axum::rejection=trace",
            env!("CARGO_CRATE_NAME")
        )
        .into()
    });

    let fmt_layer = tracing_subscriber::fmt::layer();

    match config.otlp_endpoint {
        Some(endpoint) => {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(&endpoint)
                .build()
                .expect("failed to create OTLP exporter");

            let provider = TracerProvider::builder()
                .with_batch_exporter(exporter, runtime::Tokio)
                .build();

            let tracer = provider.tracer(config.service_name);
            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .with(otel_layer)
                .init();

            tracing::info!("OpenTelemetry tracing enabled, exporting to {}", endpoint);
            Some(provider)
        }
        None => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .init();

            tracing::info!("OpenTelemetry tracing disabled (no OTEL_EXPORTER_OTLP_ENDPOINT set)");
            None
        }
    }
}

/// Shutdown the tracer provider gracefully.
pub fn shutdown_tracing(provider: Option<TracerProvider>) {
    if let Some(provider) = provider
        && let Err(e) = provider.shutdown()
    {
        tracing::error!("Failed to shutdown tracer provider: {:?}", e);
    }
}
