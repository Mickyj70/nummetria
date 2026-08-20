# Changelog

All notable changes to Nummetria will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project intends to follow Semantic Versioning once distributable CLI
releases begin.

## [Unreleased]

### Added

- Initial Nummetria product website.
- Product, architecture, privacy, scope, CLI contract, and data-model
  documentation.
- Open-source contribution, governance, conduct, security, and licensing
  policies.
- Rust workspace, cross-platform CLI command skeleton, and macOS/Windows CI.
- Provider-neutral usage records, decimal money, explicit cost evidence, and a
  versioned JSON Schema.
- SQLite migrations, transactional inserts, idempotent retries, querying,
  aggregation, checkpoints, backup, and deletion.
- Atomic versioned JSON import with validation-only dry runs and structured
  command output.
- Initial all-time `status` and normalized `usage` read-back commands.
- Round-trip JSON export and spreadsheet-friendly CSV export with safe file
  creation.
- Versioned TOML configuration, macOS and Windows standard path discovery, and
  deterministic command/environment/config/default precedence.
- Native macOS Keychain and Windows Credential Manager abstractions with
  redacted secret values and isolated test stores.
- Create-once setup, configuration inspection, automatic database discovery,
  consistent backup, and deliberately confirmed data deletion commands.
- Secret-safe configuration errors that never echo malformed file contents.
