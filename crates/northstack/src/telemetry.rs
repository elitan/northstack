//! Logging setup. JSON logs in production (`LOG_FORMAT=json`, the default),
//! human-readable logs in development (`LOG_FORMAT=pretty`). Filter with
//! `RUST_LOG`, e.g. `RUST_LOG=info,app=debug`.

use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));
    let pretty = std::env::var("LOG_FORMAT")
        .map(|v| v == "pretty")
        .unwrap_or(false);
    let registry = tracing_subscriber::registry().with(filter);
    let result = if pretty {
        registry
            .with(tracing_subscriber::fmt::layer().compact())
            .try_init()
    } else {
        registry
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_current_span(true),
            )
            .try_init()
    };
    // Already initialized (e.g. in tests) is fine.
    let _ = result;
}
