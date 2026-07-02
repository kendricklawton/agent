//! Layered configuration (12-factor): **defaults < file (TOML) < env (`AGENT_*`) < flags**. Selecting the
//! model and data-provider adapter is *config, not code* — a new adapter is reachable by name without
//! touching a call site. Secrets (API keys) come from the environment only, never from this struct or the
//! config file.
//!
//! IO and logic are separated: [`resolve`] is a pure fold over the layers (unit-tested for precedence),
//! while [`load`] does the impure env-read + file-read and then calls it.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

/// The resolved configuration the app runs with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Which model adapter to use (e.g. `"mock"`, later `"claude"`).
    pub model: String,
    /// Which data-provider adapter to use (e.g. `"mock"`, later `"polygon"`).
    pub provider: String,
    /// The `tracing` filter directive (e.g. `"warn"`, `"debug"`, `"agent=debug"`).
    pub log: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "mock".to_owned(),
            provider: "mock".to_owned(),
            log: "warn".to_owned(),
        }
    }
}

/// One layer's overrides — every field optional, so layers merge cleanly. Deserialized from the TOML file
/// and also built from env vars and CLI flags.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Partial {
    /// Override the model adapter.
    pub model: Option<String>,
    /// Override the data-provider adapter.
    pub provider: Option<String>,
    /// Override the log filter.
    pub log: Option<String>,
}

impl Partial {
    /// Overrides from the `AGENT_*` environment (empty values are treated as unset).
    fn from_env() -> Self {
        Self {
            model: env_var("AGENT_MODEL"),
            provider: env_var("AGENT_PROVIDER"),
            log: env_var("AGENT_LOG"),
        }
    }

    /// Apply this layer over `base`: every `Some` field wins, `None` leaves `base` untouched.
    fn apply(self, base: &mut Config) {
        if let Some(model) = self.model {
            base.model = model;
        }
        if let Some(provider) = self.provider {
            base.provider = provider;
        }
        if let Some(log) = self.log {
            base.log = log;
        }
    }
}

/// Read an `AGENT_*` env var, treating empty as unset.
fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

/// Fold the layers over the defaults in precedence order (lowest first): `file`, then `env`, then `flags`.
/// Pure — the whole point of the split, so precedence is testable without touching the real environment.
#[must_use]
pub fn resolve(file: Partial, env: Partial, flags: Partial) -> Config {
    let mut config = Config::default();
    file.apply(&mut config);
    env.apply(&mut config);
    flags.apply(&mut config);
    config
}

/// Load config from an optional file plus the environment, then apply the CLI `flags` on top.
///
/// The file is read only from an explicit path — the `--config` flag (`config_path`) or the `AGENT_CONFIG`
/// env var, in that order. A requested-but-unreadable/invalid file is an error; no path means no file layer
/// (there is no surprise CWD auto-scan).
///
/// # Errors
/// If a config-file path is given but can't be read or parsed as TOML.
pub fn load(flags: Partial, config_path: Option<&Path>) -> anyhow::Result<Config> {
    let path: Option<PathBuf> = config_path
        .map(Path::to_path_buf)
        .or_else(|| env_var("AGENT_CONFIG").map(PathBuf::from));

    let file = match path {
        Some(path) => {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading config file {}", path.display()))?;
            toml::from_str(&text)
                .with_context(|| format!("parsing config file {}", path.display()))?
        }
        None => Partial::default(),
    };

    Ok(resolve(file, Partial::from_env(), flags))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn partial(model: Option<&str>, provider: Option<&str>, log: Option<&str>) -> Partial {
        Partial {
            model: model.map(str::to_owned),
            provider: provider.map(str::to_owned),
            log: log.map(str::to_owned),
        }
    }

    #[test]
    fn defaults_are_mock_and_quiet() {
        let c = resolve(Partial::default(), Partial::default(), Partial::default());
        assert_eq!(c, Config::default());
        assert_eq!(
            (c.model.as_str(), c.provider.as_str(), c.log.as_str()),
            ("mock", "mock", "warn")
        );
    }

    #[test]
    fn precedence_is_flags_over_env_over_file_over_defaults() {
        let file = partial(Some("file-model"), Some("file-prov"), Some("file-log"));
        let env = partial(Some("env-model"), None, Some("env-log"));
        let flags = partial(Some("flag-model"), None, None);
        let c = resolve(file, env, flags);
        assert_eq!(c.model, "flag-model"); // flag wins
        assert_eq!(c.provider, "file-prov"); // only the file set it
        assert_eq!(c.log, "env-log"); // env over file, no flag
    }

    #[test]
    fn empty_partials_fall_through_to_defaults() {
        let c = resolve(
            partial(None, None, None),
            partial(None, None, Some("debug")),
            partial(None, None, None),
        );
        assert_eq!(c.model, "mock");
        assert_eq!(c.log, "debug");
    }

    #[test]
    fn toml_file_parses_into_a_partial() {
        let p: Partial =
            toml::from_str("model = \"claude\"\nlog = \"debug\"\n").expect("valid toml");
        assert_eq!(p.model.as_deref(), Some("claude"));
        assert_eq!(p.provider, None);
        assert_eq!(p.log.as_deref(), Some("debug"));
    }

    #[test]
    fn unknown_toml_key_is_rejected() {
        assert!(toml::from_str::<Partial>("modle = \"claude\"\n").is_err());
    }

    #[test]
    fn load_errors_on_a_missing_requested_file() {
        let missing = Path::new("/no/such/agent-config.toml");
        assert!(load(Partial::default(), Some(missing)).is_err());
    }
}
