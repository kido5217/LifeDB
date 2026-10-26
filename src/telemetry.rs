//! `tracing` subscriber wiring: EnvFilter levels, pretty-vs-JSON fmt,
//! stdout only (design D5). OTel layers can be added here later without
//! touching call sites.

use anyhow::Context;
use tracing::Subscriber;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, Registry};

use crate::config::{Config, LogFormat};

/// Install the global subscriber for the process.
pub fn init(config: &Config) -> anyhow::Result<()> {
    let filter = EnvFilter::try_new(&config.rust_log)
        .context("RUST_LOG did not parse as an EnvFilter (validated at startup)?")?;
    let layer = fmt_layer(std::io::stdout, matches!(config.log_format, LogFormat::Json), filter);
    tracing_subscriber::registry()
        .with(layer)
        .try_init()
        .map_err(|err| anyhow::anyhow!("installing the log subscriber: {err}"))?;
    Ok(())
}

/// One fmt layer, JSON when `json` is set, with its own filter attached.
fn fmt_layer<W>(writer: W, json: bool, filter: EnvFilter) -> Box<dyn Layer<Registry> + Send + Sync>
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    if json {
        Box::new(fmt::layer().json().with_writer(writer).with_filter(filter))
    } else {
        Box::new(fmt::layer().with_writer(writer).with_filter(filter))
    }
}

/// Build a subscriber writing to `writer` (JSON when `json` is set) for tests.
pub(crate) fn subscriber_for<W>(writer: W, json: bool, directive: &str) -> impl Subscriber
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let filter = EnvFilter::try_new(directive).expect("test directive parses");
    Registry::default().with(fmt_layer(writer, json, filter))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    /// In-memory MakeWriter for capturing subscriber output in tests.
    #[derive(Clone)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl MakeWriter<'_> for Buf {
        type Writer = Self;
        fn make_writer(&self) -> Self::Writer {
            self.clone()
        }
    }

    impl Write for Buf {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn captured(json: bool, emit: impl FnOnce()) -> String {
        let buf = Buf(Arc::new(Mutex::new(Vec::new())));
        let subscriber = subscriber_for(buf.clone(), json, "info");
        tracing::subscriber::with_default(subscriber, emit);
        String::from_utf8(buf.0.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn json_events_carry_request_fields() {
        let output = captured(true, || {
            let span = tracing::info_span!("request", path = "/api/v1/auth/me");
            let _guard = span.enter();
            tracing::info!(method = "GET", status = 200u16, duration_ms = 1.5f64, "request completed");
        });
        let line = output.lines().last().expect("one event line");
        let event: serde_json::Value = line.parse().expect("valid JSON line");
        assert_eq!(event["fields"]["message"], "request completed");
        assert_eq!(event["fields"]["status"], 200);
        assert_eq!(event["fields"]["duration_ms"], 1.5);
        assert_eq!(event["fields"]["method"], "GET");
        // The span context rides along so correlated lines can be joined.
        assert!(
            event["spans"].as_array().is_some_and(|s| s.iter().any(|sp| sp["name"] == "request")),
            "span context missing: {event}"
        );
        assert!(event["timestamp"].is_number() || event["timestamp"].is_string());
    }

    #[test]
    fn pretty_events_render_text() {
        let output = captured(false, || {
            tracing::info!("hello");
        });
        assert!(output.contains("hello"), "{output}");
    }
}
