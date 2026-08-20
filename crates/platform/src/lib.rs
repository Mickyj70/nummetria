//! Cross-platform configuration, paths, and secret storage.
//!
//! Operating-system-specific code stays in this crate rather than leaking into
//! commands or domain types.

mod config;
mod paths;
mod secrets;

pub use config::{
    AppConfig, ConfigError, ConfigSource, EnvironmentOverrides, ResolveOptions, ResolvedConfig,
    SetupOutcome, resolve_config, write_initial_config,
};
pub use paths::{PathError, PlatformPaths};
pub use secrets::{
    CredentialId, InMemorySecretStore, KeyringSecretStore, SecretError, SecretStore, SecretValue,
};
