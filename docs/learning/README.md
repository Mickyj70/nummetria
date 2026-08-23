# Learning notes

Nummetria is also a teaching project. Learning notes explain concepts that are
useful beyond a single pull request: Rust workspace design, command contracts,
SQLite migrations, provider adapters, secure secret handling, testing, and
cross-platform releases.

Notes use numbered filenames and follow this outline:

1. What problem are we solving?
2. What concept should you understand first?
3. How does Nummetria apply it?
4. Which trade-offs did we accept?
5. What experiment can you run yourself?

Pull requests should link a learning note when they introduce a substantial new
concept. Small changes can teach directly through the PR description instead.

## Notes

1. [Rust workspace boundaries](001-rust-workspace.md)
2. [Domain invariants](002-domain-invariants.md)
3. [Safe SQLite persistence](003-sqlite-storage.md)
4. [A trustworthy import pipeline](004-import-pipeline.md)
5. [Cross-platform configuration without leaking secrets](005-platform-configuration.md)
6. [Normalize a provider API without inventing data](006-openai-provider.md)
7. [Prove a core is provider-neutral with a second adapter](007-anthropic-provider.md)
