# Roadmap

The roadmap describes intent, not a promise of dates. Each phase should land as
a focused pull request with tests and documentation.

## v0.1 — CLI foundation

- [x] Initial product website.
- [ ] Repository foundation and open-source policies.
- [x] Rust workspace and command skeleton.
- [x] Provider-neutral domain model and versioned schemas.
- [ ] SQLite storage, migrations, deduplication, and checkpoints.
- [ ] JSON import plus JSON and CSV export.
- [ ] Cross-platform configuration and native credential storage.
- [ ] OpenAI provider adapter.
- [ ] Anthropic provider adapter.
- [ ] Usage reports and local budgets.
- [ ] Diagnostics, completions, backup, and deletion.
- [ ] macOS and Windows release artifacts and installation documentation.

## After v0.1

The next product phase will validate the shared core through a macOS menu bar
experience. Notch-specific interaction will be an optional presentation mode,
not a separate source of data or business logic. A Windows tray experience will
follow the same shared-core approach.

Other candidates include scheduled collection, notifications, additional
providers, pricing-table updates, and opt-in local integrations. Each requires a
separate design and privacy review before entering a release milestone.

See [docs/v0.1-scope.md](docs/v0.1-scope.md) for the binding initial scope.
