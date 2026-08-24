# Local Codex source

The local Codex source imports token-count metadata written by Codex on the
same computer. It requires no OpenAI API or organization admin key. It does not
claim to measure ChatGPT subscription limits, remaining quota, or monetary
cost.

## Support status

OpenAI's public Codex documentation does not define a stable local rollout-file
schema. Nummetria therefore treats this integration as a version-detected local
adapter, not a public OpenAI API contract. Unknown structures are reported as
unsupported and never guessed.

The initial implementation supports JSON Lines rollouts containing session
metadata, turn context with a model, and `token_count` events with
`last_token_usage` fields for input, cached input, cache-write input, output,
reasoning output, and total tokens.

## Commands

```text
nummetria sources codex detect [--codex-home <PATH>]
nummetria sources codex status [--codex-home <PATH>]
nummetria collect codex [--codex-home <PATH>]
                         [--start <YYYY-MM-DD>]
                         [--end <YYYY-MM-DD>]
```

An explicit `--codex-home` wins. Otherwise Nummetria checks `CODEX_HOME`, then
the current user's `.codex` directory. Detection is read-only and reports only
support status, the resolved source directory, and counts; it never prints
session content.

Dates are UTC and half-open. Collection reads matching rollout files in stable
path order and emits records in event timestamp and identity order. A partially
written trailing JSON line is ignored with a warning so collection can safely
run while Codex is active.

## Incremental collection

An unbounded `collect codex` run uses a versioned SQLite checkpoint to resume
each rollout at its last complete JSONL byte boundary. The checkpoint stores
only the relative rollout name, byte offset, line number, session identity, and
last model metadata required to normalize appended events. It never stores an
absolute Codex path or any prompt, response, tool, reasoning, or shell content.

Before resuming, Nummetria verifies the rollout's typed session header. A
missing, truncated, replaced, or mismatched file is safely read again from its
beginning; SQLite idempotency prevents duplicate records. A partial trailing
line is not included in the offset and is retried on the next collection.
Files absent from the current source are removed from the next checkpoint.

Explicit `--start` or `--end` collection is a historical query: it performs a
full scan and does not read or advance the incremental checkpoint. This avoids
marking filtered-out observations as consumed.

Usage records and the new checkpoint commit in one SQLite transaction. A
malformed rollout, invalid observation, storage conflict, or failed checkpoint
write leaves both records and the previous checkpoint unchanged. Invalid or
unsupported checkpoint JSON fails closed to a full rescan with a sanitized
warning.

Collection summaries report rollout files discovered, files resumed, files
reset, complete lines examined, usage observations read, records inserted, and
records already present. These counters contain no source content.

## Normalization

Each non-empty `last_token_usage` event becomes one immutable observation.
Input, cached-input, cache-write-input, output, and reasoning-output values map
to their corresponding Nummetria quantities, with one request per non-empty
event. The most recent preceding turn model is retained when available.

Nummetria does not store the Codex working directory, repository remote,
branch, title, first message, preview, prompt, response, reasoning text, tool
arguments, tool results, or shell output.

The event timestamp becomes a one-millisecond observation interval. Identity
derives from session ID, event timestamp, and numeric usage. Recollection is
idempotent. Cost remains `unknown`: subscription activity is not API billing,
so API list prices are not applied.

## Privacy boundary

Rollouts may contain sensitive content beside usage events. The collector must
use typed streaming deserialization and skip unknown payloads without
constructing generic JSON values. Ignored content is never returned from the
source crate, logged, stored, exported, or included in errors. Canary-secret
fixtures verify this boundary.

The collector never opens `auth.json`, attachments, shell snapshots,
transcription history, or unrelated Codex databases. It never modifies a Codex
file.

## Failure and compatibility

- A missing Codex directory is a normal `not_found` detection result.
- No supported rollouts is `unsupported` or `empty`, not successful zero usage.
- Malformed completed lines produce relative, line-numbered errors without
  reproducing content.
- Invalid records leave Nummetria SQLite unchanged.
- Checkpoints advance only after every selected rollout is read successfully.
- Local source errors use exit `3`; Nummetria storage errors use exit `4`.

A new Codex layout requires a sanitized fixture, documentation update, and
privacy review. Detection fails closed when required metadata changes.

## Official references

OpenAI documents Codex and supported configuration, but currently does not
document the rollout schema used by this adapter:

- [Codex documentation](https://developers.openai.com/codex/)
- [Codex configuration](https://developers.openai.com/codex/config-basic/)
