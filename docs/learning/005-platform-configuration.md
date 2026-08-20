# 005 — Cross-platform configuration without leaking secrets

## What problem are we solving?

A command-line tool should work without making users repeat paths on every
command, but it must also remain predictable in scripts and safe around API
credentials. macOS and Windows use different standard directories and native
secret stores, so scattering platform checks throughout commands would make
the behavior difficult to test and maintain.

## Separate policy from mechanism

Nummetria keeps platform mechanisms in `crates/platform`. That crate discovers
standard directories, parses versioned TOML, resolves overrides, and provides a
native credential-store interface. The CLI decides when those mechanisms are
used and how results are presented.

This boundary gives the rest of the workspace ordinary Rust types such as
`PathBuf`, `ResolvedConfig`, and a `SecretStore` trait. Provider and reporting
code do not need to know whether a credential came from macOS Keychain or
Windows Credential Manager.

## Precedence is part of the public contract

Configuration is not merely a file parser. It is a deterministic decision:

1. A command option is the most explicit choice.
2. An environment variable supports automation and CI.
3. TOML stores a durable user preference.
4. An operating-system default makes the common case effortless.

The resolver returns both the selected value and its source. That is why
`config show` can explain a surprising path instead of forcing users to guess.
Relative database paths in TOML are anchored to the configuration directory,
so the same file behaves consistently regardless of the current shell folder.

## Secrets are a different kind of setting

An API key is not accepted in TOML. Native credential stores protect secret
values, while configuration contains only non-secret choices. The secret value
wrapper deliberately renders as `[REDACTED]` through both `Debug` and `Display`;
code must make an explicit call to expose the value at the narrow point where
an authenticated request will eventually need it.

Even a rejected configuration can leak data if a parser error repeats the
offending source line. Nummetria therefore converts TOML parse errors into a
sanitized CLI message that names the file without echoing its contents. A
regression test writes a fake key into an invalid configuration and proves the
value never reaches standard error.

## Safe setup and destructive operations

`setup` creates standard directories and uses create-new persistence for the
initial file. Repeating it keeps the existing configuration. It does not create
a database because merely inspecting or initializing paths should not mutate
stored usage.

Backup also uses create-new behavior, so an existing file is never replaced.
Deletion requires `--all` to state the scope and either an exact interactive
confirmation or `--yes` for automation. It deletes data inside the schema
rather than removing the database file, and it leaves configuration and native
credentials untouched.

## Cross-platform tests without touching real credentials

Tests inject temporary configuration and data directories instead of changing
the developer's real home folders. The secret-store contract uses an in-memory
implementation; automated tests never open Keychain or Credential Manager.
Binary-level tests run the actual CLI process to verify environment precedence,
exit codes, JSON stream separation, backup refusal, deletion, and redaction.

## Try it yourself

Create the standard directories and inspect every resolved source:

```bash
cargo run --bin nummetria -- setup
cargo run --bin nummetria -- config show
cargo run --bin nummetria -- data path
```

Try a temporary override without editing TOML:

```bash
NUMMETRIA_DATABASE=./experiment.db cargo run --bin nummetria -- data path
```

Run the focused tests:

```bash
cargo test -p nummetria-platform
cargo test -p nummetria-cli --test platform_workflow
```

As an experiment, add an unknown `api_key` field to a temporary TOML file and
run `config validate`. The command rejects the field, returns exit code `2`, and
does not repeat the value in its error output.
