# Contributing to Nummetria

Thank you for helping build Nummetria. Contributions should make the project
easier to trust, use, maintain, or learn from.

## Before you begin

- Read the [product definition](docs/product.md),
  [v0.1 scope](docs/v0.1-scope.md), and
  [architecture](docs/architecture.md).
- Search existing issues and pull requests.
- Open or comment on an issue before starting a large or user-visible change.
- Never include real credentials, provider payloads, prompts, or responses in
  fixtures, logs, screenshots, or discussions.

## Branch workflow

All changes enter `main` through a pull request. Use a short-lived branch:

- `feat/<description>` for a feature.
- `fix/<description>` for a bug fix.
- `docs/<description>` for documentation.
- `chore/<description>` for tooling or maintenance.

Make small, coherent commits using Conventional Commit subjects, for example:

```text
feat(cli): add status command skeleton
fix(storage): keep imports idempotent
docs: explain cost evidence
```

Checkpoint commits are encouraged while learning. Pull requests are squash
merged so `main` retains one clear commit for each completed change.

## Pull requests

A pull request must explain:

1. What changed?
2. Why is it needed?
3. How does it work?
4. How was it tested?
5. What can another contributor learn from it?

Keep documentation and tests in the same pull request as behavior. Avoid mixing
unrelated refactors with a feature or fix.

## Local checks

For Rust changes:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For website changes:

```bash
npm install
npm run lint
npm test
```

Run both groups when a change crosses the website and CLI boundary. Pull request
titles must also use Conventional Commit syntax.

## Review expectations

- Be specific and kind.
- Discuss behavior and trade-offs, not people.
- Treat privacy, secret handling, migrations, and public output formats as
  high-risk areas that require tests.
- Resolve requested changes before merging.

By participating, you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
