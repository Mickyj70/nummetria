# Import and export

Nummetria v0.1 exchanges normalized usage through a versioned JSON envelope:

```json
{
  "format_version": 1,
  "records": []
}
```

Each item in `records` must satisfy the
[`UsageRecord` v1 schema](../schemas/usage-record-v1.schema.json). The envelope
rejects unknown fields and unsupported format versions. An empty record array
is a valid no-op. Imports preserve each record's collection source so a JSON
export can be imported without changing its identity or provenance.

## Import behavior

```text
nummetria --database <PATH> import <FILE>
nummetria import <FILE> --dry-run
```

Validation is atomic. Nummetria reads the envelope, validates every record, and
reports all record-level failures using locations such as `records[2]`. If any
record is invalid, Nummetria does not open or change the database. A conflict
between an incoming record and an existing record with the same ID also rolls
back the entire batch.

`--dry-run` validates only the file. It does not open SQLite, detect existing
duplicates, or detect conflicts with stored records. Its summary makes that
limitation explicit.

Successful imports report the number of records read, validated, inserted, and
already present. Reimporting an identical exchange is safe and inserts no new
rows.

## Read-back and export

```text
nummetria --database <PATH> status
nummetria --database <PATH> usage
nummetria --database <PATH> export --format json
nummetria --database <PATH> export --format csv
nummetria --database <PATH> export --format <json|csv> --output <PATH>
```

`status` prints all-time quantities and costs grouped by evidence and currency.
`usage` lists normalized records in period-start and record-ID order. Date
ranges and custom grouping belong to the later reporting milestone.

JSON export writes the same exchange envelope accepted by `import`. CSV uses
one row per usage record. It includes identity, period, provider, cost, source,
and a fixed column for every v0.1 usage kind. Multiple quantities of the same
kind in one record are summed with exact decimal arithmetic.

Export writes to standard output unless `--output <PATH>` is supplied. An
existing destination is never overwritten. `--quiet` does not suppress export
data because the data is the command's essential output.

## Machine output and exit codes

With `--json`, command results use this envelope on standard output:

```json
{
  "output_version": 1,
  "command": "import",
  "data": {},
  "warnings": []
}
```

Errors use a versioned envelope on standard error with an error code, message,
and optional indexed details. Human-readable errors also go to standard error.

| Code | Meaning |
| ---: | --- |
| `0` | Success |
| `2` | Invalid arguments or import data |
| `3` | File input or output failure |
| `4` | SQLite or storage failure |

Nummetria never imports or exports prompts, responses, or credentials.
