# Anthropic provider

Nummetria's Anthropic adapter reads organization-level Messages usage and cost
reports. It never requests or stores prompts, responses, message bodies, or
credential values. This adapter covers the Anthropic API organization; a
Claude Pro or Max subscription is a separate product and is not represented by
these reports.

## Account requirement

Anthropic's Usage and Cost Admin API requires an Admin API key created in the
Anthropic Console. A normal inference API key is not sufficient. Nummetria
stores the admin key under the provider/profile identity `anthropic/default` in
macOS Keychain or Windows Credential Manager.

```text
nummetria providers anthropic set-key [--profile <NAME>]
nummetria providers anthropic set-key [--profile <NAME>] --from-stdin
nummetria providers anthropic status [--profile <NAME>]
nummetria providers anthropic delete-key [--profile <NAME>] [--yes]
```

Interactive entry is hidden. Automation may read a key from standard input,
but a credential is never accepted as a command argument. Status reveals only
whether a credential exists, and deliberate deletion never prints the value.

## Collection command

```text
nummetria collect anthropic [--profile <NAME>]
                              [--start <YYYY-MM-DD>]
                              [--end <YYYY-MM-DD>]
```

Dates are UTC and form a half-open interval. Start is inclusive, end is
exclusive, and end must be later than start. By default, collection ends at the
current UTC day boundary and resumes from the last successful checkpoint, or
starts 30 days earlier when no checkpoint exists.

The first adapter collects daily buckets from:

- `GET /v1/organizations/usage_report/messages`, grouped by workspace and
  model;
- `GET /v1/organizations/cost_report`, grouped by workspace and description.

Every request includes the documented `anthropic-version` header. Pagination
follows `has_more` and `next_page`; `has_more` without a cursor is a provider
contract error. Authentication failures are not retried. Rate limits,
temporary network failures, and server failures use the same bounded retry
policy as other provider adapters.

## Normalization

Message usage becomes provider-neutral quantities for uncached input, output,
cache reads, cache writes, and reported server-tool requests. Cache creation
durations are summed into the core cache-write quantity while their detailed
breakdown remains provider metadata when present.

Usage observations have unknown cost evidence. Cost-report rows become
separate observations with reported monetary values and no invented token
allocation. This separation prevents a workspace-level invoice amount from
being misrepresented as an exact model-level cost.

Stable identities use only the provider stream, UTC bucket, grouping fields,
and currency. A repeated collection is idempotent. If Anthropic later returns
different data under the same identity, storage reports a conflict rather than
silently rewriting history.

## Atomic storage and checkpoints

Nummetria fetches and validates the full requested range before changing
SQLite. Usage records, cost records, and both profile-specific checkpoints are
committed in one transaction. A response, normalization, HTTP, or storage
failure leaves both records and checkpoints unchanged. Historical collection
never moves a newer checkpoint backwards.

## Output and failures

Success reports the UTC range, pages fetched, observations read, inserted and
existing records, and resulting checkpoints. `--json` uses the versioned CLI
response envelope.

- exit `2`: invalid date, range, profile, or missing credential;
- exit `3`: credential-store or local path failure;
- exit `4`: SQLite failure or record conflict;
- exit `5`: provider authentication, rate limit, HTTP, response, or pagination
  failure.

Provider errors are sanitized. Headers, admin keys, response bodies, prompts,
and responses never enter logs, SQLite, exports, diagnostics, or errors.

## Official API references

- [Anthropic Messages usage report](https://platform.claude.com/docs/en/api/admin/usage_report/retrieve_messages)
- [Anthropic cost report](https://platform.claude.com/docs/en/api/admin/cost_report/retrieve)
