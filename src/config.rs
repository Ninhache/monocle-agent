//! Resolved telemetry configuration for the Monocle OTLP exporter.

use opentelemetry::KeyValue;

/// Default OTLP ingestion endpoint for Monocle cloud.
const DEFAULT_ENDPOINT: &str = "https://ingest.monocle.sh";
/// Default deployment environment reported to Monocle when `MONOCLE_ENV` is unset.
const DEFAULT_ENV: &str = "production";
/// Fallback tracing filter used when `RUST_LOG` is not set.
const DEFAULT_LOG_FILTER: &str = "info";

/// Configuration for the Monocle telemetry exporter.
///
/// Build it from the environment with [`MonocleConfig::from_env`] (the usual
/// path) and tweak it with the `with_*` builder methods. Export stays **off**
/// until an API key is present ([`MonocleConfig::is_enabled`]).
///
/// ```
/// let cfg = monocle_agent::MonocleConfig::from_env(
///     env!("CARGO_PKG_NAME"),
///     env!("CARGO_PKG_VERSION"),
/// )
/// .with_environment("staging");
/// ```
#[derive(Clone, Debug)]
pub struct MonocleConfig {
    /// Monocle API key. `None` disables all OTLP export (stdout logging only).
    pub api_key: Option<String>,
    /// OTLP/HTTP base endpoint. Per-signal paths (`/v1/traces`, …) are appended internally.
    pub endpoint: String,
    /// Deployment environment (`development` | `staging` | `production`).
    pub environment: String,
    /// `service.name` resource attribute.
    pub service_name: String,
    /// `service.version` resource attribute.
    pub service_version: String,
    /// Fallback tracing filter used when `RUST_LOG` is unset (e.g. `"my_svc=debug,info"`).
    pub log_filter: String,
    /// Extra OpenTelemetry resource attributes, merged on top of the defaults.
    pub extra_resource_attributes: Vec<KeyValue>,
}

impl MonocleConfig {
    /// Resolve configuration from environment variables.
    ///
    /// `service_name` / `service_version` are the fallbacks used when the
    /// matching env vars are unset — pass `env!("CARGO_PKG_NAME")` and
    /// `env!("CARGO_PKG_VERSION")` so they resolve to *your* crate (the `env!`
    /// macro must expand in the calling crate, not in this library).
    ///
    /// | Env var | Field | Default |
    /// |---------|-------|---------|
    /// | `MONOCLE_API_KEY` | `api_key` | `None` (export disabled) |
    /// | `MONOCLE_ENDPOINT` | `endpoint` | `https://ingest.monocle.sh` |
    /// | `MONOCLE_ENV` | `environment` | `production` |
    /// | `OTEL_SERVICE_NAME` | `service_name` | `service_name` arg |
    /// | `MONOCLE_SERVICE_VERSION` | `service_version` | `service_version` arg |
    pub fn from_env(service_name: impl Into<String>, service_version: impl Into<String>) -> Self {
        Self {
            // Treat an empty value the same as unset everywhere.
            api_key: env_nonempty("MONOCLE_API_KEY"),
            endpoint: env_nonempty("MONOCLE_ENDPOINT")
                .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()),
            environment: env_nonempty("MONOCLE_ENV").unwrap_or_else(|| DEFAULT_ENV.to_string()),
            service_name: env_nonempty("OTEL_SERVICE_NAME").unwrap_or_else(|| service_name.into()),
            service_version: env_nonempty("MONOCLE_SERVICE_VERSION")
                .unwrap_or_else(|| service_version.into()),
            log_filter: DEFAULT_LOG_FILTER.to_string(),
            extra_resource_attributes: Vec::new(),
        }
    }

    /// Override the API key (and thereby enable export).
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        let key = key.into();
        self.api_key = if key.trim().is_empty() {
            None
        } else {
            Some(key)
        };
        self
    }

    /// Override the OTLP/HTTP base endpoint.
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// Override the deployment environment.
    #[must_use]
    pub fn with_environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = environment.into();
        self
    }

    /// Override the fallback tracing filter (used only when `RUST_LOG` is unset).
    #[must_use]
    pub fn with_log_filter(mut self, filter: impl Into<String>) -> Self {
        self.log_filter = filter.into();
        self
    }

    /// Append an extra OpenTelemetry resource attribute.
    #[must_use]
    pub fn with_resource_attribute(
        mut self,
        key: impl Into<opentelemetry::Key>,
        value: impl Into<opentelemetry::Value>,
    ) -> Self {
        self.extra_resource_attributes
            .push(KeyValue::new(key.into(), value.into()));
        self
    }

    /// Whether OTLP export is enabled (i.e. an API key is present).
    pub fn is_enabled(&self) -> bool {
        self.api_key.is_some()
    }
}

/// Read an env var, treating an empty/whitespace value the same as unset.
fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_api_key_enables_export() {
        let cfg = MonocleConfig::from_env("svc", "1.0").with_api_key("secret");
        assert!(cfg.is_enabled());
        assert_eq!(cfg.api_key.as_deref(), Some("secret"));
    }

    #[test]
    fn blank_api_key_stays_disabled() {
        let cfg = MonocleConfig::from_env("svc", "1.0").with_api_key("   ");
        assert!(!cfg.is_enabled());
    }

    #[test]
    fn builder_overrides_apply() {
        let cfg = MonocleConfig::from_env("svc", "1.0")
            .with_endpoint("http://localhost:4318")
            .with_environment("staging")
            .with_resource_attribute("team", "platform");
        assert_eq!(cfg.endpoint, "http://localhost:4318");
        assert_eq!(cfg.environment, "staging");
        assert_eq!(cfg.extra_resource_attributes.len(), 1);
    }
}
