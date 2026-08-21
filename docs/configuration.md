# Configuration, paths, and credentials

Nummetria uses operating-system-standard directories, a versioned TOML file for
non-secret settings, and the native credential store for secrets.

## Configuration file

Version 1 has one optional setting:

```toml
config_version = 1

# Optional. Relative paths are resolved from this configuration file's folder.
database_path = "data/nummetria.db"
```

Unknown fields and unsupported versions are errors. The default configuration
file may be absent; defaults still work. A path selected explicitly with
`--config` or `NUMMETRIA_CONFIG` must exist and be valid.

API keys, tokens, passwords, prompts, and responses are never valid
configuration fields. Provider credentials live only in macOS Keychain or
Windows Credential Manager. Provider profiles select native credential entries
without placing credential values in configuration.

## Resolution and precedence

The configuration file is selected in this order:

1. `--config <PATH>`
2. `NUMMETRIA_CONFIG`
3. the operating-system configuration directory

The SQLite database is selected in this order:

1. `--database <PATH>`
2. `NUMMETRIA_DATABASE`
3. `database_path` in the selected TOML file
4. `nummetria.db` in the operating-system local-data directory

An empty environment value is invalid rather than silently ignored. Command
options and environment paths are resolved from the process working directory.
Only a TOML `database_path` is relative to the TOML file.

On macOS, standard directories are beneath the user's `Library` folders. On
Windows, configuration uses the roaming application-data folder and the
database uses local application data. `nummetria config path` and
`nummetria data path` always show the exact resolved locations.

## Commands

```text
nummetria setup
nummetria config path
nummetria config show
nummetria config validate
nummetria data path
nummetria data backup --output <PATH>
nummetria data delete --all
nummetria data delete --all --yes
```

`setup` creates the standard configuration and data directories and writes a
minimal versioned configuration only when it is missing. It never overwrites an
existing file and does not create the database.

`config show` reports resolved paths and the source of each value. JSON output
uses the normal versioned command envelope. No secret or credential-store value
is read by these inspection commands.

Backups use SQLite's consistent backup operation and refuse to overwrite an
existing destination. Data deletion prints the resolved database target and
requires an interactive confirmation. `--yes` is the explicit non-interactive
confirmation. JSON mode requires `--yes` so a machine-readable command never
pauses for terminal input. Deletion clears usage and collection checkpoints
while retaining the schema and configuration; credentials are not deleted with
usage data.

## Errors and privacy

Invalid configuration and missing confirmation use exit code `2`, file and
path failures use `3`, and SQLite failures use `4`. Human errors go to standard
error; `--json` emits the versioned error envelope there.

Secrets are wrapped in a redacted value type. Debug output, display output,
configuration, SQLite, exports, errors, and diagnostics must never contain the
secret value. Automated tests use a fake credential store and never read or
change a developer's native credentials.
