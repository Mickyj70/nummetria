# 004 — Build a trustworthy import pipeline

## What problem are we solving?

An import command accepts data the program did not create in its current
process. The file may be malformed, use a future format, contain several bad
records, or conflict with data already stored. A useful importer must explain
those failures without leaving a half-written database.

## Deserialization is not the same as validation

Deserialization turns JSON syntax into Rust values. Validation decides whether
those values satisfy Nummetria's rules. Serde can do both when domain types use
custom deserializers and `deny_unknown_fields`, but the exchange parser still
uses two deliberate stages:

1. Parse and validate the file-level envelope and its format version.
2. Deserialize each raw record independently into a validated `UsageRecord`.

The second stage lets Nummetria collect errors for several record indexes in one
run instead of stopping at the first bad record. The database is not opened
until every record passes.

## Atomicity and idempotency solve different problems

Atomicity means a batch either commits completely or changes nothing. If the
tenth record conflicts with stored data, the first nine do not remain inserted.

Idempotency means retrying an identical import is safe. A record with the same
ID and identical canonical payload is reported as already present. Reusing the
ID for different contents is a conflict, because silently replacing it would
make usage history unreliable.

The dry-run path validates without opening SQLite. This is stronger than
rolling a transaction back: even database creation, migrations, and pragmas are
avoided. The trade-off is that a dry run cannot predict stored duplicates or
conflicts, so the command reports that limitation.

## Stable formats are public APIs

The exchange envelope has its own `format_version`, while each record has a
`schema_version`. This separation allows the container and the domain record to
evolve independently. JSON export writes the same envelope accepted by import,
which gives us a direct round-trip test for information loss.

Machine-readable command results use a third version, `output_version`. Scripts
can therefore distinguish command summaries from portable exchange files.
Human-readable text may improve without silently changing the automation
contract.

CSV is intentionally one row per record. Usage kinds become fixed columns and
multiple quantities of one kind are summed with decimal arithmetic. A record's
cost appears once, preventing the common spreadsheet mistake of double-counting
cost after expanding quantities into several rows.

## Cross-platform behavior

Paths remain `Path` and `PathBuf` values until they must be displayed. Tests use
temporary paths containing spaces and Unicode, and the same binary-level suite
runs on macOS and Windows. Export files use create-new behavior rather than a
check-then-write sequence, so another process cannot slip an existing file into
the gap and have it overwritten.

The CLI separates failure classes with stable exit codes: invalid input, file
I/O, and storage errors are different outcomes for shell scripts. Structured
errors go to standard error, leaving standard output safe for exported data.

## Try it yourself

Validate the sample without creating a database:

```bash
cargo run --bin nummetria -- import fixtures/exchange/valid-v1.json --dry-run
```

Then import, inspect, and round-trip it:

```bash
cargo run --bin nummetria -- --database ./nummetria-demo.db import fixtures/exchange/valid-v1.json
cargo run --bin nummetria -- --database ./nummetria-demo.db status
cargo run --bin nummetria -- --database ./nummetria-demo.db export --format json
```

Run the binary-level workflow tests with:

```bash
cargo test -p nummetria-cli --test data_workflow
```

As an experiment, change one field while keeping the sample record ID. Import
the original first and the changed file second; the second batch will fail as a
conflict without changing the stored data.
