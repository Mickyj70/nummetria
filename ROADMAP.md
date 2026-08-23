# Roadmap

The roadmap describes intent, not a promise of dates. Each phase should land as
a focused pull request with tests and documentation.

## v0.1 — CLI foundation

- [x] Initial product website.
- [x] Repository foundation and open-source policies.
- [x] Rust workspace and command skeleton.
- [x] Provider-neutral domain model and versioned schemas.
- [x] SQLite storage, migrations, deduplication, and checkpoints.
- [x] JSON import plus JSON and CSV export.
- [x] Cross-platform configuration and native credential storage.
- [x] OpenAI provider adapter.
- [x] Anthropic provider adapter.
- [ ] Privacy-reviewed local Codex and Claude Code sources.
- [ ] Usage reports and local budgets.
- [ ] Manual subscription renewal tracking.
- [ ] Explainable local usage anomaly detection.
- [x] Database backup and deliberate data deletion.
- [ ] Diagnostics and shell completions.
- [ ] macOS and Windows release artifacts and installation documentation.

## After v0.1

The next product phase will validate the shared core through a macOS menu bar
experience. Notch-specific interaction will be an optional presentation mode,
not a separate source of data or business logic. A Windows tray experience will
follow the same shared-core approach.

Other candidates include Gemini and xAI provider adapters, scheduled
collection, notifications, pricing-table updates, and more opt-in local
integrations. Each requires a separate design and privacy review before entering
a release milestone.

See [docs/v0.1-scope.md](docs/v0.1-scope.md) for the binding initial scope.
