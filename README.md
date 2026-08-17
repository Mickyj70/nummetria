# Nummetria

Nummetria is a local-first, open-source command-line tool for understanding how
you use AI services. It brings usage, requests, tokens, costs, and budgets from
multiple providers into one private, consistent view.

> [!IMPORTANT]
> Nummetria is in early development. The landing page exists, but the CLI is not
> ready to install yet. Follow the [v0.1 roadmap](ROADMAP.md) to track progress.

## Why Nummetria?

AI usage is usually scattered across provider dashboards, exports, and billing
pages. Nummetria is designed to make that information:

- **Unified:** one command model across providers.
- **Explainable:** every cost is marked as reported, calculated, estimated, or
  unknown.
- **Private:** prompts, responses, and credentials are never stored.
- **Portable:** the CLI targets both macOS and Windows.
- **Extensible:** provider adapters share a documented, testable contract.

## Planned v0.1 experience

```console
$ nummetria status
Today       $4.21     182 requests     1.3M tokens
This month  $38.90    1,904 requests   12.8M tokens

$ nummetria usage --month --group-by provider
Provider    Cost      Requests         Tokens
OpenAI      $24.10    1,120            8.1M
Anthropic   $14.80      784            4.7M
```

The public command contract is documented in
[docs/cli-contract.md](docs/cli-contract.md). Commands shown above describe the
target for v0.1 and may not be implemented yet.

## Repository layout

Nummetria is a monorepo. The existing web landing page stays at the repository
root so its deployment remains stable. Rust applications and shared crates will
live under `apps/` and `crates/`.

```text
nummetria/
├── app/                 # Product website
├── public/              # Website assets
├── apps/cli/            # Cross-platform CLI (planned)
├── crates/              # Reusable Rust libraries (planned)
├── schemas/             # Versioned exchange formats (planned)
├── fixtures/            # Sanitized test data (planned)
└── docs/                # Product and engineering documentation
```

## Website development

The current repository contains the product website. Until the Rust workspace
lands, these are the available development commands:

```bash
npm install
npm run dev
npm run lint
npm test
```

Node.js 22.13 or newer is required.

## Project principles

- Local SQLite is the default source of truth.
- Native operating-system credential stores protect provider secrets.
- Collection is explicit in v0.1; no hidden daemon or telemetry runs.
- Provider-specific behavior stays behind provider adapters.
- Machine-readable output is a supported interface, not an afterthought.
- Documentation and tests change alongside behavior.

Read [docs/product.md](docs/product.md),
[docs/architecture.md](docs/architecture.md), and
[docs/privacy.md](docs/privacy.md) before proposing a large feature.

## Contributing

Nummetria welcomes contributors of every experience level. Start with
[CONTRIBUTING.md](CONTRIBUTING.md), then open an issue before beginning a large
change. Each pull request should be small, tested, documented, and useful as a
learning artifact.

## Security

Please do not report credential exposure or other vulnerabilities in a public
issue. Follow [SECURITY.md](SECURITY.md) instead.

## License

Licensed under the [Apache License 2.0](LICENSE).
