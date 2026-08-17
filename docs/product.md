# Product definition

## Vision

Nummetria gives individuals and small teams one trustworthy, private view of
their AI-tool usage. The CLI is the product foundation; desktop surfaces will
later consume the same core and storage layers.

## Primary users

- Developers using several AI APIs and coding tools.
- Independent builders who need to understand costs before invoices arrive.
- Small teams that want portable reports without adopting another cloud
  dashboard.
- Contributors learning Rust, CLI design, data modeling, and cross-platform
  engineering through a real open-source project.

## Core jobs

1. Collect usage and cost evidence from supported providers or imports.
2. Normalize provider-specific data without hiding its origin or confidence.
3. Answer simple questions quickly: what did I use, what did it cost, and how
   does that compare with my budget?
4. Export the user's own normalized data in stable, documented formats.
5. Diagnose configuration and collection problems without exposing secrets.

## Product principles

### Local first

The user's device is the default system of record. Cloud accounts are data
sources, not Nummetria accounts.

### Evidence over false precision

A provider-reported cost is different from a cost calculated from a price
table. Nummetria preserves and displays that distinction.

### Explicit collection

Version 0.1 collects only when the user runs a command. Background services and
notifications are later features with separate consent and lifecycle design.

### Cross-platform core

macOS and Windows are supported by the CLI before work begins on the menu bar,
notch, or tray experience.

### Learn in public

Architecture decisions, command behavior, and data formats are documented so a
new contributor can understand not only what was built, but why.

## Success criteria for v0.1

- A new user can install Nummetria on macOS or Windows and complete setup.
- JSON usage data can be imported repeatedly without duplicate records.
- OpenAI and Anthropic usage can be collected through documented credentials.
- Usage can be reported by time range, provider, model, and project.
- Budget checks and exports work without network access after collection.
- Credentials, prompts, and responses never appear in stored data or output.
- Every supported command has human-readable and JSON behavior documented.
