//! Structured logging: `tracing` events rendered to **stderr**, filtered by the resolved [`Config`]. stdout
//! is reserved for the answer / `--json` (12-factor XI: logs are an event stream, not part of the output).

use tracing_subscriber::EnvFilter;

use crate::config::Config;

/// Initialize the global `tracing` subscriber: a fmt layer to **stderr**, filtered by `config.log` (our
/// `AGENT_LOG`, e.g. `"warn"` or `"agent=debug"`). A malformed filter falls back to `warn` rather than
/// failing the run. Safe to call once at startup; a second call is a no-op (`try_init`).
pub fn init(config: &Config) {
    let filter = EnvFilter::try_new(&config.log).unwrap_or_else(|_| EnvFilter::new("warn"));
    // `try_init` errors only if a subscriber is already set (e.g. a test called us twice) — ignore that.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_infallible_on_a_default_config() {
        // Must not panic even if called more than once.
        init(&Config::default());
        init(&Config::default());
    }
}
