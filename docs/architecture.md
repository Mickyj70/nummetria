# Architecture

## Overview

Nummetria uses a layered Rust architecture so the command-line interface and
future desktop applications share the same behavior.

```text
CLI commands
    │
    ▼
Application services
    │
    ├── Domain types and policies
    ├── Provider adapter contract ── OpenAI / Anthropic
    ├── Storage contract ─────────── SQLite
    └── Platform contract ────────── Keychain / Credential Manager / paths
```

Dependencies point inward: providers, SQLite, and operating-system integrations
may depend on core contracts, while core domain code never depends on a specific
provider, database, terminal, or desktop framework.

## Planned workspace boundaries

- `apps/cli`: argument parsing, terminal presentation, exit codes, and command
  orchestration.
- `crates/core`: provider-neutral domain types, validation, aggregation, budget
  rules, and service contracts.
- `crates/storage`: SQLite migrations and repository implementations.
- `crates/platform`: configuration paths and native secret storage.
- `crates/providers`: provider contract plus OpenAI and Anthropic adapters.

The boundaries may become separate crates gradually. They should not be split
solely to create more packages; a boundary earns a crate when it has a clear
contract and independent tests.

## Data flow

1. A command resolves configuration and credentials.
2. An importer or provider adapter produces validated domain records.
3. Storage inserts records using deterministic identities and a transaction.
4. A collection checkpoint advances only after the transaction commits.
5. Reports query normalized data and retain cost-evidence labels.
6. The CLI renders either human-readable output or a versioned JSON envelope.

## Reliability rules

- Imports and collection retries are idempotent.
- Timestamps are stored in UTC and converted only for display and user ranges.
- Money uses decimal values with an explicit ISO currency code.
- Schema changes use ordered migrations and preserve supported user data.
- Provider pagination and retry state cannot advance past uncommitted records.
- Errors shown to users are actionable and scrubbed of credentials.

## Decision records

Significant, difficult-to-reverse decisions will be recorded under
`docs/decisions/` using short Architecture Decision Records. Each record states
the context, decision, consequences, and alternatives considered.
