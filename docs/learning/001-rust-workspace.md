# 001 — Why Nummetria starts with a Rust workspace

## What problem are we solving?

Nummetria begins as a CLI, but the product plan also includes macOS menu bar,
notch, and Windows tray experiences. If business logic lives directly inside
terminal commands, every later interface must either invoke the CLI as a child
process or duplicate its behavior.

## The concept: dependency direction

A layered application keeps policy at the center and replaceable technology at
the edges. Terminal rendering, SQLite, HTTP providers, and operating-system
credential stores are edge technologies. Usage records, budgets, aggregation,
and collection rules are product policy.

Dependencies should point from an edge toward stable contracts, not from the
core toward every framework that happens to use it.

## How Nummetria applies it

- `apps/cli` owns argument parsing, terminal output, and exit codes.
- `crates/core` will own provider-neutral domain and application contracts.
- `crates/storage` will implement persistence contracts with SQLite.
- `crates/platform` will implement paths, configuration, and secret storage.
- `crates/providers` will translate provider APIs into core records.

The placeholder crates compile from day one. This lets continuous integration
enforce dependency direction before features make accidental coupling tempting.

## Trade-offs

A workspace introduces several manifests and names earlier than a single-crate
project would. We accept that small cost because the future application surfaces
and provider adapters are already part of the product direction.

We do not split every module into a crate. A crate should represent a meaningful
boundary with a testable contract, not merely make the repository look modular.

## Try it yourself

Run:

```bash
cargo metadata --no-deps --format-version 1
cargo run --bin nummetria -- --help
```

The first command shows Cargo's view of every workspace package. The second
shows how Clap derives a user-facing command contract from typed Rust data.
