# Read local usage without collecting conversation content

## What problem are we solving?

Codex stores useful token counters beside highly sensitive conversation and
tool content. Nummetria needs the counters without turning a usage tracker into
a second conversation archive.

## What concept should you understand first?

A privacy boundary is strongest when unwanted data has no representation in
the program. Typed deserialization lets Rust select timestamps, identifiers,
models, and numeric counters while discarding unknown JSON fields. Redaction is
still useful for errors, but it is weaker than never retaining content.

Incremental readers also need a transaction boundary. A byte offset is only
safe after every record derived before that offset commits. Advancing the
offset first could permanently skip data after a crash.

## How does Nummetria apply it?

The Codex adapter reads only JSONL rollouts below `sessions`. It normalizes
non-empty token-count events into provider-neutral records with unknown cost.
It never opens credentials, attachments, shell snapshots, or Codex databases.

For normal collection, a versioned cursor remembers relative file names,
complete-line byte offsets, line numbers, session identity, and current model.
The source verifies the immutable header before resuming. Truncated or replaced
files restart safely, and SQLite deduplication handles records seen again.
Records and the cursor commit in one transaction.

Explicit date ranges are different: they are historical queries, so they scan
all rollouts and never change synchronization state.

## Which trade-offs did we accept?

- The local rollout schema is undocumented, so compatibility is detected and
  fails closed rather than guessed.
- Subscription quota and monetary cost remain unknown.
- Detection still parses typed metadata; it cannot promise support without
  inspecting the structure.
- v0.1 collection is explicit and has no background watcher.

## What experiment can you run?

Use a fresh database and collect twice:

```bash
test_directory="$(mktemp -d)"
cargo run -p nummetria-cli -- --database "$test_directory/usage.db" --json collect codex
cargo run -p nummetria-cli -- --database "$test_directory/usage.db" --json collect codex
```

The second run should resume every unchanged rollout and examine zero lines.
Then run a historical query and observe that `checkpoint_advanced` is false.
