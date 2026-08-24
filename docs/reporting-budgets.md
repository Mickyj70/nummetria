# Usage reporting and local budgets

## Reporting commands

Bare `usage` keeps its v0.1 compatibility behavior and lists normalized
records. Aggregated reporting is explicit:

```text
nummetria usage report [--period <today|week|month|all>]
                        [--start <YYYY-MM-DD>] [--end <YYYY-MM-DD>]
                        [--group-by <none|provider|model|project|source>]
```

The default report period is the current UTC month and the default grouping is
`none`. Today is the current UTC calendar day, week begins Monday, and month is
the UTC calendar month. All ranges are half-open. `--start` and `--end` form a
custom range and conflict with `--period`; both custom bounds are required.

Reports sum quantities with exact decimal arithmetic. Costs remain separated
by evidence and currency; Nummetria never converts currencies or silently
combines reported, calculated, and estimated evidence. Unknown-cost record
counts are always shown. Missing model or project values group as `unknown`.
Canonical source keys are `provider_api`, `local:<tool>`, and `import`.

Human output is compact. `--json` emits decimal strings, explicit UTC range
bounds, grouping keys, evidence-separated costs, and unknown-cost counts in the
versioned CLI envelope. Stable ordering is grouping key, quantity kind, cost
evidence, then currency.

## Budget commands

Budgets are local SQLite records and never trigger background activity in
v0.1:

```text
nummetria budget create <NAME> --amount <DECIMAL> --currency <ISO-4217>
                               --period <daily|weekly|monthly>
                               [--provider <ID>] [--model <ID>]
                               [--project <ID>] [--source <KEY>]
nummetria budget list
nummetria budget check [NAME] [--at <YYYY-MM-DD>]
nummetria budget delete <NAME> [--yes]
```

Names are unique and non-empty. Amounts are positive exact decimals and
currencies use uppercase ISO 4217 codes. Period windows follow UTC calendar
boundaries; `--at` selects the containing window for reproducible checks and
defaults to the current UTC date. Filters are combined with logical AND.

A check sums matching known costs only in the budget currency while retaining
an evidence breakdown. It also reports matching unknown-cost records and known
cost records in other currencies. Nummetria performs no currency conversion.

Budget state is:

- `exceeded` when known matching cost is greater than the limit;
- `indeterminate` when the known cost is not over the limit but unknown or
  other-currency matching costs prevent a trustworthy within-budget claim;
- `within` only when the complete matching cost is known in the budget
  currency and does not exceed the limit.

Equality with the limit is `within`. Checks report the exact limit, known
amount, remaining amount (never below zero), evidence breakdown, unknown count,
other-currency count, and evaluated UTC window.

Deletion requires an interactive confirmation. `--yes` is required with
`--json`. Creating a duplicate name or deleting an unknown name is an invalid
input error. Usage records and budgets are independent: deleting usage does not
delete budget definitions, and deleting a budget never deletes usage.

## Storage and safety

The budget migration stores decimal amounts as text, timestamps as canonical
UTC strings, and optional provider/model/project/source filters. Reads use the
same overlap semantics as usage queries. Database failures use exit code `4`;
invalid options and domain values use exit code `2`.
