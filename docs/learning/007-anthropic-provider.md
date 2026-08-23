# 007 — Prove a core is provider-neutral with a second adapter

## What problem are we solving?

An abstraction can look provider-neutral when only one provider uses it. The
real test arrives when a second API has different authentication, timestamps,
grouping dimensions, quantities, and response shapes.

Anthropic reports usage in RFC 3339 buckets, separates cache creation by
duration, and identifies workspaces rather than OpenAI projects. Its Admin API
uses `x-api-key` plus an explicit API-version header. Nummetria must preserve
those facts without leaking them into its core domain or inventing equivalence
where none exists.

## Contract tests versus duplicated implementation

A provider contract defines observable behavior: collect a half-open UTC
range, follow every page, normalize supported quantities, retain cost evidence,
sanitize failures, and produce deterministic records. Each adapter implements
that behavior using its provider's protocol.

The OpenAI and Anthropic adapters intentionally share domain and storage types,
not raw response structs. Anthropic response types remain private to the
provider crate. This prevents an API rename from becoming a database migration.

## How Anthropic data is normalized

- `uncached_input_tokens` becomes `input_tokens`.
- `output_tokens` becomes `output_tokens`.
- cache-read tokens become `cached_tokens`.
- five-minute and one-hour cache-creation tokens are summed as
  `cache_write_tokens`.
- reported web-search requests become `web_searches`.
- a workspace ID occupies the provider-neutral project dimension.

Usage rows retain unknown cost evidence. Cost-report rows remain separate,
reported monetary observations. Nummetria does not allocate a workspace-level
cost to a model merely because both appeared during the same day.

## Why collection is atomic

Usage and cost pagination completes before SQLite changes. Both streams and
their checkpoints commit in one transaction. A later failure cannot leave a
successful usage checkpoint beside missing costs. Repeating the same range
inserts nothing; changed content under the same deterministic identity becomes
a visible conflict.

## Secret boundaries

The CLI stores an Anthropic Admin API key in macOS Keychain or Windows
Credential Manager. Tests use an in-memory credential store and sanitized mock
HTTP server. They verify that the key reaches the adapter but never terminal
output or SQLite.

Claude Pro and Max subscriptions are not Anthropic API organizations. This
adapter therefore cannot claim to measure consumer subscription limits or
Claude chat usage.

## Experiments to run

Run the provider contract tests:

```bash
cargo test -p nummetria-providers anthropic
```

Run the CLI security and checkpoint tests:

```bash
cargo test -p nummetria-cli anthropic
```

Inspect the public surface:

```bash
cargo run --bin nummetria -- providers anthropic --help
cargo run --bin nummetria -- collect anthropic --help
```

With an Anthropic organization Admin API key, store it through the hidden
prompt and collect a small historical range:

```bash
cargo run --bin nummetria -- providers anthropic set-key
cargo run --bin nummetria -- --database ./anthropic-test.db \
  collect anthropic --start 2026-08-01 --end 2026-08-03
```

The [Anthropic provider contract](../providers/anthropic.md) contains the
complete compatibility and privacy rules.
