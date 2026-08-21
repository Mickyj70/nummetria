# 006 — Normalize a provider API without inventing data

## What problem are we solving?

Provider dashboards combine several concepts that do not necessarily share the
same dimensions. OpenAI can report completion usage by project and model, while
its Costs API reports spend by project and line item. Joining those rows as if
the API supplied model-level costs would create numbers that look precise but
are not supported by evidence.

## Preserve evidence before presentation

The adapter creates two observation streams. Completion rows contain token and
request quantities with unknown cost. Cost rows contain reported money and no
usage quantities. Reports can sum both correctly without allocating spend to a
model unless a future, documented calculation deliberately does so.

This required a small provider-neutral domain improvement: an observation is
valid when it has quantities, known cost evidence, or both. An observation with
neither remains invalid. The JSON schema and Rust constructor enforce the same
rule.

## External JSON is not a domain model

OpenAI response structs are private to the provider crate. Each bucket is
converted into validated `UsageRecord` values, and raw response bodies are then
dropped. Provider-specific fields used for identity or provenance are encoded
in deterministic IDs and operation metadata rather than storing an unbounded
payload column.

The record ID includes the stream, bucket bounds, grouping values, and currency
where relevant. Repeating the same response therefore produces the same IDs.
If a provider later changes a historical value under one of those identities,
SQLite reports a conflict instead of silently rewriting history.

## Pagination and retry are separate concerns

Pagination follows `has_more` and `next_page`. A response that promises another
page without supplying a cursor is rejected because guessing could skip data or
loop forever.

Retries apply only to transport failures, rate limits, and server failures.
Authentication and other client errors return immediately. The retry policy is
bounded and injectable, allowing tests to use three immediate attempts instead
of sleeping. Error messages include the failure class or HTTP status, never the
authorization header or response body.

## Commit records and progress together

Fetching every page happens before SQLite changes. The storage layer then
inserts all observations and advances both stream checkpoints in one
transaction. A conflicting record rolls back new records and checkpoints.

Checkpoints contain the exclusive UTC end date, not a temporary page cursor.
Page cursors are useful only during one API traversal; a date boundary is a
stable place for the next collection to resume. Explicit historical collection
never moves a checkpoint backwards.

## Test the contract, not the provider account

The provider tests use a localhost mock server with sanitized fixtures. They
verify authorization shape, exact query parameters, multiple pages, costs,
rate-limit retries, missing cursors, and redacted errors. CLI tests inject an
in-memory credential store and collector to prove orchestration, idempotency,
and atomic checkpoint behavior without touching Keychain, Credential Manager,
or the internet.

## Try it yourself

Run the safe mock tests:

```bash
cargo test -p nummetria-providers
cargo test -p nummetria-cli openai
```

Inspect the commands without storing a credential:

```bash
cargo run --bin nummetria -- providers openai --help
cargo run --bin nummetria -- collect openai --help
```

If you have an OpenAI organization admin key, store it through the hidden
prompt and collect complete UTC days:

```bash
cargo run --bin nummetria -- providers openai set-key
cargo run --bin nummetria -- collect openai --start 2026-08-01 --end 2026-08-03
```

The [OpenAI provider contract](../providers/openai.md) explains account access,
defaults, output, and removal. Never paste a real key into a command argument,
configuration file, issue, test fixture, or terminal transcript.
