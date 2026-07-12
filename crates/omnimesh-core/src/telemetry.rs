//! Structured logging and tracing initialization.
//!
//! Uses the `tracing` ecosystem for structured, filterable logs.
//! Supports both human-readable and JSON output formats.

use tracing_subscriber::{fmt, EnvFilter};

/// Initialize the global tracing subscriber.
///
/// Call this once at application startup. The filter level can be
/// controlled via:
/// 1. The `log_level` parameter (e.g., "info", "debug", "omnimesh=trace")
/// 2. The `RUST_LOG` environment variable (overrides `log_level`)
///
/// # Arguments
/// * `log_level` — Default filter string (e.g., "info")
/// * `json` — If true, output logs in JSON format (for machine consumption)
pub fn init(log_level: &str, json: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    if json {
        let subscriber = fmt::Subscriber::builder()
            .with_env_filter(filter)
            .json()
            .flatten_event(true)
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("failed to set global tracing subscriber");
    } else {
        let subscriber = fmt::Subscriber::builder()
            .with_env_filter(filter)
            .with_target(true)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("failed to set global tracing subscriber");
    }
}

/// Create a tracing span for a specific subsystem.
///
/// # Example
/// ```ignore
/// let _guard = omnimesh_core::telemetry::span("transport", "quic_connect");
/// ```
pub fn span(subsystem: &str, operation: &str) -> tracing::span::Entered<'static> {
    let span = tracing::info_span!("omnimesh", subsystem = subsystem, operation = operation,);
    // Leak the span so the Entered guard has a 'static lifetime.
    // This is acceptable for long-lived subsystem spans.
    let leaked: &'static tracing::Span = Box::leak(Box::new(span));
    leaked.enter()
}
