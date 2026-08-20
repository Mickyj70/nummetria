//! Cross-platform configuration, paths, and secret storage.
//!
//! Operating-system-specific code stays in this crate rather than leaking into
//! commands or domain types.

mod paths;

pub use paths::{PathError, PlatformPaths};
