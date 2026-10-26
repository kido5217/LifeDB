//! Environment-only configuration, fail-fast (design D10).
//!
//! Every runtime knob arrives through one environment variable. All
//! problems are collected so a misconfigured process reports every
//! missing or invalid variable at once, then refuses to start.

use std::{env, fmt, time::Duration};
use thiserror::Error;
use tracing_subscriber::filter::EnvFilter;

/// Deployment target; drives cookie `Secure` defaults and log format expectations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Dev,
    Prod,
}

/// Request log encoding (stdout only; the process manager captures).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub listen_addr: String,
    pub environment: Environment,
    pub log_format: LogFormat,
    pub rust_log: String,
    pub allowed_origins: Vec<String>,
    pub cookie_secure: bool,
    pub body_limit_bytes: usize,
    pub request_timeout: Duration,
    pub session_ttl: Duration,
}

/// Startup configuration failure listing every problem found in the environment.
#[derive(Debug, Clone, Error)]
#[error("invalid configuration:\n{}", self.problems.join("\n"))]
pub struct ConfigError {
    pub problems: Vec<String>,
}

impl Config {
    /// Read configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Parse configuration from an injected lookup (unit-testable; `None` = unset).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let mut problems = Vec::new();

        let required = |name: &str, fallback: Option<String>, problems: &mut Vec<String>| {
            lookup(name).or(fallback).unwrap_or_else(|| {
                problems.push(format!("{name}: missing (required)"));
                String::new()
            })
        };

        let database_url = required("DATABASE_URL", None, &mut problems);
        let listen_addr = required("LISTEN_ADDR", Some("127.0.0.1:8080".into()), &mut problems);

        let environment = match lookup("ENVIRONMENT").as_deref() {
            None | Some("dev") => Environment::Dev,
            Some("prod") => Environment::Prod,
            Some(other) => {
                problems.push(format!("ENVIRONMENT: expected dev|prod, got {other:?}"));
                Environment::Dev
            }
        };

        let log_format = match lookup("LOG_FORMAT").as_deref() {
            None => {
                if environment == Environment::Prod {
                    LogFormat::Json
                } else {
                    LogFormat::Pretty
                }
            }
            Some("pretty") => LogFormat::Pretty,
            Some("json") => LogFormat::Json,
            Some(other) => {
                problems.push(format!("LOG_FORMAT: expected pretty|json, got {other:?}"));
                LogFormat::Pretty
            }
        };

        let rust_log = lookup("RUST_LOG").unwrap_or_else(|| DEFAULT_RUST_LOG.to_string());
        if EnvFilter::try_new(&rust_log).is_err() {
            problems.push(format!("RUST_LOG: not a valid EnvFilter: {rust_log:?}"));
        }

        let allowed_origins = match lookup("ALLOWED_ORIGINS") {
            None => {
                problems.push("ALLOWED_ORIGINS: missing (required)".to_string());
                Vec::new()
            }
            Some(raw) => {
                let list: Vec<String> = raw
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if list.is_empty() {
                    problems.push(
                        "ALLOWED_ORIGINS: no usable entries (comma-separated list)".to_string(),
                    );
                }
                for entry in &list {
                    if axum::http::HeaderValue::from_str(entry).is_err() {
                        problems.push(format!(
                            "ALLOWED_ORIGINS: not a usable origin value: {entry:?}"
                        ));
                    }
                }
                list
            }
        };

        let cookie_secure = match lookup("COOKIE_SECURE").as_deref() {
            None => environment == Environment::Prod,
            Some("true") => true,
            Some("false") => false,
            Some(other) => {
                problems.push(format!("COOKIE_SECURE: expected true|false, got {other:?}"));
                false
            }
        };

        let body_limit_bytes = parse_positive(
            &lookup("BODY_LIMIT_BYTES"),
            2 * 1024 * 1024,
            "BODY_LIMIT_BYTES",
            &mut problems,
        ) as usize;
        let request_timeout = Duration::from_secs(parse_positive(
            &lookup("REQUEST_TIMEOUT_SECS"),
            30,
            "REQUEST_TIMEOUT_SECS",
            &mut problems,
        ));
        let session_ttl = Duration::from_secs(parse_positive(
            &lookup("SESSION_TTL_SECS"),
            7 * 24 * 3600,
            "SESSION_TTL_SECS",
            &mut problems,
        ));

        if !problems.is_empty() {
            return Err(ConfigError { problems });
        }
        Ok(Self {
            database_url,
            listen_addr,
            environment,
            log_format,
            rust_log,
            allowed_origins,
            cookie_secure,
            body_limit_bytes,
            request_timeout,
            session_ttl,
        })
    }
}

const DEFAULT_RUST_LOG: &str = "lifedb=info,tower_http=info,sqlx=warn";

fn parse_positive(
    raw: &Option<String>,
    default: u64,
    name: &str,
    problems: &mut Vec<String>,
) -> u64 {
    match raw {
        None => default,
        Some(text) => match text.parse::<u64>() {
            Ok(v) if v > 0 => v,
            _ => {
                problems.push(format!("{name}: expected a positive integer, got {text:?}"));
                default
            }
        },
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dev => f.write_str("dev"),
            Self::Prod => f.write_str("prod"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn minimal() -> Vec<(&'static str, &'static str)> {
        vec![
            ("DATABASE_URL", "postgres://postgres@localhost:5433/lifedb"),
            ("ALLOWED_ORIGINS", "http://localhost:5173"),
        ]
    }

    #[test]
    fn missing_database_url_is_named() {
        let err = Config::from_lookup(lookup_from(&[("ALLOWED_ORIGINS", "http://x")])).unwrap_err();
        assert!(
            err.problems.iter().any(|p| p.contains("DATABASE_URL")),
            "{err}"
        );
    }

    #[test]
    fn multiple_bad_vars_are_reported_together() {
        let err = Config::from_lookup(lookup_from(&[
            ("REQUEST_TIMEOUT_SECS", "soon"),
            ("ENVIRONMENT", "staging"),
            ("LOG_FORMAT", "yaml"),
        ]))
        .unwrap_err();
        for name in [
            "DATABASE_URL",
            "ALLOWED_ORIGINS",
            "REQUEST_TIMEOUT_SECS",
            "ENVIRONMENT",
            "LOG_FORMAT",
        ] {
            assert!(
                err.problems.iter().any(|p| p.contains(name)),
                "missing report for {name}: {err}"
            );
        }
    }

    #[test]
    fn defaults_apply_when_optional_vars_are_unset() {
        let cfg = Config::from_lookup(lookup_from(&minimal())).unwrap();
        assert_eq!(cfg.listen_addr, "127.0.0.1:8080");
        assert_eq!(cfg.body_limit_bytes, 2 * 1024 * 1024);
        assert_eq!(cfg.request_timeout, Duration::from_secs(30));
        assert_eq!(cfg.session_ttl, Duration::from_secs(7 * 24 * 3600));
        assert_eq!(cfg.log_format, LogFormat::Pretty);
        assert!(!cfg.cookie_secure);
    }

    #[test]
    fn prod_implies_secure_cookie_unless_overridden() {
        let mut pairs = vec![("ENVIRONMENT", "prod")];
        pairs.extend(minimal());
        let cfg = Config::from_lookup(lookup_from(&pairs)).unwrap();
        assert!(cfg.cookie_secure);
        let mut pairs = vec![
            ("ENVIRONMENT", "prod"),
            ("COOKIE_SECURE", "false"),
            ("LOG_FORMAT", "pretty"),
        ];
        pairs.extend(minimal());
        let cfg = Config::from_lookup(lookup_from(&pairs)).unwrap();
        assert!(!cfg.cookie_secure);
    }

    #[test]
    fn malformed_origin_value_is_named_not_panicked() {
        let err = Config::from_lookup(lookup_from(&[
            ("DATABASE_URL", "postgres://x"),
            ("ALLOWED_ORIGINS", "http://good.example,ht\ntp://broken"),
        ]))
        .unwrap_err();
        assert!(
            err.problems
                .iter()
                .any(|p| p.contains("ALLOWED_ORIGINS") && p.contains("broken")),
            "{err}"
        );
    }

    #[test]
    fn zero_timeouts_and_empty_origin_lists_are_rejected() {
        let err = Config::from_lookup(lookup_from(&[
            ("DATABASE_URL", "postgres://x"),
            ("ALLOWED_ORIGINS", " , "),
            ("SESSION_TTL_SECS", "0"),
        ]))
        .unwrap_err();
        assert!(err.problems.iter().any(|p| p.contains("ALLOWED_ORIGINS")));
        assert!(err.problems.iter().any(|p| p.contains("SESSION_TTL_SECS")));
    }
}
