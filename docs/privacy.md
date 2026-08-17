# Privacy model

Nummetria is designed to answer usage and cost questions without becoming a new
collector of sensitive AI conversations.

## Data Nummetria may store

- Provider, model, project, and account-scoped identifiers needed for reports.
- Usage quantities such as tokens, requests, images, or compute time.
- Cost values, currencies, and evidence classification.
- Collection timestamps, source references, and synchronization checkpoints.
- Budgets and non-secret user preferences.

## Data Nummetria must not store

- Prompt or response content.
- API keys, access tokens, passwords, or credential-store values.
- Provider HTTP authorization headers.
- Hidden product telemetry or advertising identifiers.

## Credential handling

Secrets are stored only through macOS Keychain or Windows Credential Manager.
Configuration files contain secret references, never secret values. Credentials
must be redacted from logs, errors, diagnostics, database rows, and exports.

## Network behavior

Network requests occur only for an explicit provider operation initiated by the
user in v0.1. Import, reporting, budgets, backup, deletion, and local export do
not require network access.

Nummetria itself sends no analytics or telemetry. Provider requests are still
subject to the provider's own privacy policy and account configuration.

## User control

Users can inspect the configuration and database locations, export normalized
records, create a backup, remove provider credentials, and delete local data.
Destructive commands must identify their target and require deliberate
confirmation unless an explicit non-interactive confirmation option is used.

## Security reports

Potential secret exposure or privacy failures should be reported according to
[SECURITY.md](../SECURITY.md), not through a public issue.
