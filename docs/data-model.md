# Data model direction

The domain model is provider-neutral while preserving enough source evidence to
audit how a number was produced.

## Core concepts

- `UsageRecord`: an immutable observation for a provider, time interval, and
  optional model or project.
- `UsageQuantity`: a typed amount such as input tokens, output tokens, cached
  tokens, requests, images, audio seconds, tool calls, or compute seconds.
- `Money`: a decimal amount paired with an ISO 4217 currency code.
- `CostEvidence`: reported, calculated, estimated, or unknown.
- `CollectionSource`: provider API, imported file, or future supported source.
- `TimeRange`: a half-open UTC interval with an inclusive start and exclusive
  end.

## Invariants

- Record identities are deterministic for the same source observation.
- Quantities cannot be negative.
- Currency is never inferred silently across records.
- A calculated or estimated cost records the pricing reference used.
- Provider payloads are not stored as an escape hatch for missing modeling.
- Exchange formats carry a schema version.

Exact serialized shapes will be added with the core-domain implementation and
published under `schemas/`.
