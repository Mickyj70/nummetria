# 003 — Make local persistence safe to retry

## What problem are we solving?

Imports and provider collection can be interrupted. A retry must not double the
user's usage or cost, and a partial batch must not leave the database in a
state that looks complete.

## The concept: transactions and idempotence

A transaction makes a group of database changes atomic: either every change
commits or none of them does. Idempotence means repeating an operation has the
same final effect as performing it once.

Nummetria inserts each batch inside one SQLite transaction. A repeated record
with the same deterministic ID and identical payload is counted as already
present. The same ID with different contents is an error, because silently
accepting it could hide an importer bug or changed provider observation.

## How migrations work

The database records its schema version with SQLite's `user_version` pragma.
Ordered SQL migration files move older databases forward inside transactions.
A build refuses to open a database created by a newer schema version it does
not understand.

Migration files are history. Once released, an existing migration must not be
rewritten; a schema change gets the next numbered file.

## Why values have two representations

The complete versioned `UsageRecord` JSON is the canonical value used for
lossless round trips. Query dimensions are projected into columns so SQLite can
filter by provider, model, project, and UTC interval without decoding every
payload. Quantities are also stored as child rows for future reporting queries.

Money and quantities stay decimal strings. Timestamps are normalized to UTC in
a fixed RFC 3339 form. This avoids binary floating-point rounding and makes
timestamp text sort chronologically.

## Cross-platform and safety choices

The Rust SQLite dependency bundles SQLite, giving macOS and Windows builds the
same database capabilities instead of depending on whichever system library is
installed. Foreign keys are enabled on every connection, busy writes wait for
a short bounded period, and write-ahead logging improves normal local read/write
behavior.

Backups use SQLite's online backup API and refuse to overwrite an existing
destination. Deletion keeps the schema intact, and foreign keys remove child
quantities with their parent record.

## Trade-offs

Keeping a canonical payload plus query projections duplicates a small amount of
data and requires transactional writes. In return, domain round trips remain
simple while common queries have typed, indexed columns. The first aggregation
implementation runs over validated records in Rust; later reporting work can
move proven hot paths into SQL without changing the public storage contract.

## Try it yourself

Run the focused storage tests:

```bash
cargo test -p nummetria-storage
```

Then change the cost on the second insert in the idempotence test. The operation
will become a conflict instead of an accepted retry. You can also inspect the
query plan for an indexed provider/time lookup:

```sql
EXPLAIN QUERY PLAN
SELECT payload
FROM usage_records
WHERE provider = 'openai' AND period_start < '2026-09-01T00:00:00.000000000Z';
```
