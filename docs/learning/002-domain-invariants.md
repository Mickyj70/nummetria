# 002 — Make invalid usage data hard to represent

## What problem are we solving?

Provider APIs disagree about names, units, time buckets, and cost detail. If
Nummetria passes loosely typed JSON through every layer, each report and export
must repeatedly guess whether values are valid.

## The concept: domain invariants

An invariant is a rule that must remain true for every valid value. A Rust type
can enforce rules at its construction boundary so later code operates on trusted
values instead of defensive guesses.

Nummetria rejects empty identifiers, reversed or empty time ranges, negative
usage quantities, records without quantities, invalid currency codes, and
unsupported schema versions.

## Why decimals are strings in JSON

Money and billable quantities must not use binary floating-point arithmetic.
For example, many decimal fractions cannot be represented exactly by an IEEE
754 `f64`. Nummetria calculates with `rust_decimal` and serializes decimal
values as strings, preserving the provider's precision across languages.

## Why cost evidence is part of the type

A number reported on an invoice is not equivalent to one calculated from a
price table. The `Cost` enum forces every record to say whether its cost is
reported, calculated, estimated, or unknown. Calculated and estimated variants
also retain their pricing reference.

## Trade-offs

Validated constructors and custom deserialization add code. In return, storage,
reports, and exports share one definition of valid data, and malformed imports
fail at a clear boundary.

## Try it yourself

Open `fixtures/usage/valid-v1.json`, change a quantity to `"-1"`, and run:

```bash
cargo test -p nummetria-core
```

The fixture test will fail because deserialization uses the same constructor
rules as provider adapters.
