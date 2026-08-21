# CLI contract

This document defines the intended public command surface for v0.1. Behavior
will be filled in command by command before each implementation lands.

## Commands

```text
nummetria
├── setup
├── status
├── collect
├── usage
├── providers
├── sources
├── budget
├── subscription
├── anomaly
├── import
├── export
├── config
├── data
├── doctor
├── completion
└── version
```

## Global options

- `--json`: emit the versioned machine-readable response envelope.
- `--quiet`: suppress non-essential success and progress output.
- `--no-color`: disable ANSI color even when the terminal supports it.
- `--config <PATH>`: use an explicit TOML configuration file.
- `--database <PATH>`: use an explicit SQLite database file.
- `--verbose`: include diagnostic progress without revealing secrets.

## Compatibility rules

- Human-readable presentation may improve between minor releases.
- Documented JSON field meanings will not change incompatibly within v0.x
  without a schema-version change.
- Success uses exit code `0`; invalid input, configuration failure, unavailable
  providers, partial collection, and internal failure will receive documented
  non-zero codes before their commands are implemented.
- Interactive commands must have an explicit non-interactive form suitable for
  automation.
- Secret values are never accepted through output-producing diagnostics.

## Command groups

- `setup` initializes paths and guides provider configuration.
- `status` gives the fastest current summary.
- `collect` requests new usage from configured providers.
- `usage` queries and groups stored usage.
- `providers` manages and tests provider connections.
- `sources` discovers and manages opt-in local collectors.
- `budget` creates and checks local budgets.
- `subscription` tracks manually entered recurring plans and renewal dates.
- `anomaly` checks, lists, and explains deterministic usage anomalies.
- `import` validates and stores supported exchange files.
- `export` writes normalized JSON or CSV data.
- `config` inspects and changes non-secret configuration.
- `data` reports paths and performs backup or deliberate deletion.
- `doctor` diagnoses installation, storage, and provider health.
- `completion` generates shell completion scripts.
- `version` prints version and build information.

## Implemented local-data contract

The v0.1 JSON exchange, atomic import behavior, initial `status` and `usage`
read-back, export formats, output envelopes, and exit codes are defined in
[Import and export](import-export.md). These behaviors are compatibility
contracts from the first implementation onward.

Configuration precedence, operating-system paths, native credential handling,
setup, configuration inspection, backup, and deliberate data deletion are
defined in [Configuration, paths, and credentials](configuration.md).

The OpenAI credential, collection, normalization, pagination, retry, and
checkpoint contract is defined in [OpenAI provider](providers/openai.md).

The matching organization-level contract for Anthropic usage and costs is
defined in [Anthropic provider](providers/anthropic.md).
