# OpenAI provider

Nummetria's first live provider adapter reads organization-level usage and
costs from OpenAI. It never requests or stores prompts, responses, request
bodies, or credential values.

## Account requirement

The organization Usage and Costs APIs require an OpenAI admin API key. A normal
project API key is not sufficient. Nummetria stores the admin key under the
provider/profile identity `openai/default` in macOS Keychain or Windows
Credential Manager. Profiles allow a later release to support more than one
organization without changing the credential contract.

```text
nummetria providers openai set-key [--profile <NAME>]
nummetria providers openai set-key [--profile <NAME>] --from-stdin
nummetria providers openai status [--profile <NAME>]
nummetria providers openai delete-key [--profile <NAME>] [--yes]
```

The normal `set-key` flow reads a hidden terminal value. `--from-stdin` is the
explicit automation path and never accepts a key in an argument. Status reports
only whether a credential exists. Deletion requires confirmation and never
prints the deleted value.

## Collection command

```text
nummetria collect openai [--profile <NAME>]
                           [--start <YYYY-MM-DD>]
                           [--end <YYYY-MM-DD>]
```

Dates are UTC and form a half-open interval: start is inclusive and end is
exclusive. The end must be later than the start. The default end is the current
UTC day boundary, so incomplete daily buckets are not stored. The default start
is the saved checkpoint, or 30 days before the end when no checkpoint exists.
Explicit dates override checkpoint-derived defaults.

The first adapter collects:

- `GET /v1/organization/usage/completions`, grouped by `project_id` and
  `model`, using daily buckets;
- `GET /v1/organization/costs`, grouped by `project_id` and `line_item`, using
  daily buckets.

Each request follows `has_more` and `next_page` until complete. A missing cursor
while `has_more` is true is a provider-contract error. Authentication failures
are not retried. Rate limits and server failures are retried at most three
times with bounded delay; tests use a no-delay policy and mock HTTP servers.

## Normalization

Completion results become usage observations with input, output, cached, cache
write, and request quantities when reported. Their cost evidence is `unknown`
because the Costs API does not attribute spend to model-level usage rows.

Cost results become separate cost observations with no usage quantities and a
`reported` monetary value. Keeping them separate prevents invented model-level
allocations and double-counting. Cost line items remain in the deterministic
record identity and provider operation metadata; project IDs are retained.

Record identities are derived only from the provider stream, UTC bucket,
grouping fields, and currency. They never contain credentials. Repeating an
identical range is idempotent; changed provider data under the same identity is
reported as a conflict rather than silently replacing history.

## Atomic storage and checkpoints

Nummetria fetches and validates every page before changing SQLite. Usage and
cost observations plus both stream checkpoints are committed in one
transaction. Any validation error, HTTP failure, or identity conflict leaves
records and checkpoints unchanged.

After a successful collection, each checkpoint stores the exclusive end date.
The next collection resumes from that boundary. An explicit historical range
can be recollected without moving a checkpoint backwards.

## Output and failures

Success reports the requested range, pages fetched, observations read,
records inserted, records already present, and resulting checkpoints. `--json`
uses the versioned CLI response envelope.

- exit `2`: invalid date/range/profile or missing credential;
- exit `3`: credential-store or local path failure;
- exit `4`: SQLite failure or record conflict;
- exit `5`: provider authentication, rate limit, HTTP, response, or pagination
  failure.

All provider errors are sanitized. Authorization headers, admin keys, response
bodies, prompts, and responses never enter logs, SQLite, exports, diagnostics,
or terminal errors.

## Official API references

- [OpenAI completions usage endpoint](https://developers.openai.com/api/reference/resources/admin/subresources/organization/subresources/usage/methods/completions)
- [OpenAI organization costs endpoint](https://developers.openai.com/api/reference/resources/admin/subresources/organization/subresources/usage/methods/costs)
