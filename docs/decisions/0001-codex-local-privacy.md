# ADR 0001: Extract Codex usage without collecting conversation content

## Status

Accepted for the v0.1 local Codex source.

## Context

Codex writes token-count metadata locally, but the same JSON Lines rollouts can
contain prompts, responses, reasoning, tool activity, and shell output. The
format is not a documented public OpenAI interface and may change.

## Decision

Nummetria will read only rollouts below the selected Codex `sessions` directory
using typed streaming deserialization. It retains only session identity, Codex
version, current model, event timestamp, and numeric token counts. It stores no
absolute source path or working directory, assigns unknown monetary cost, and
fails closed on unsupported structures. Canary-secret fixtures and SQLite and
export scans enforce the boundary.

## Consequences

The collector can report local token and request activity without an API key.
It cannot report remaining subscription quota, exact cost per request, prompt
analytics, repository names, or history that Codex no longer retains.

Every newly supported Codex format requires a compatibility and privacy review.

## Alternatives considered

- Aggregate thread rows cannot safely create immutable records for active
  threads.
- Generic JSON values unnecessarily materialize sensitive content.
- API prices would misrepresent subscription usage.
- UI scraping is fragile and outside v0.1 scope.
