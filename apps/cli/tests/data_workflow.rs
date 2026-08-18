use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/exchange")
        .join(name)
}

fn nummetria(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nummetria"))
        .args(args)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn write_exchange(path: &Path, records: Vec<Value>) {
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({
            "format_version": 1,
            "records": records,
        }))
        .unwrap(),
    )
    .unwrap();
}

fn valid_record() -> Value {
    let exchange: Value =
        serde_json::from_str(&fs::read_to_string(fixture("valid-v1.json")).unwrap()).unwrap();
    exchange["records"][0].clone()
}

fn import(database: &Path, exchange: &Path) -> Output {
    nummetria(&[
        "--database",
        database.to_str().unwrap(),
        "import",
        exchange.to_str().unwrap(),
    ])
}

#[test]
fn valid_empty_duplicate_and_dry_run_imports_are_predictable() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("usage data Ω.db");

    let first = import(&database, &fixture("valid-v1.json"));
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    assert!(text(&first.stdout).contains("1 inserted, 0 already present"));

    let duplicate = import(&database, &fixture("valid-v1.json"));
    assert_eq!(duplicate.status.code(), Some(0));
    assert!(text(&duplicate.stdout).contains("0 inserted, 1 already present"));

    let empty_database = directory.path().join("empty.db");
    let empty = import(&empty_database, &fixture("empty-v1.json"));
    assert_eq!(empty.status.code(), Some(0), "{}", text(&empty.stderr));
    assert!(text(&empty.stdout).contains("0 record(s)"));

    let dry_run_database = directory.path().join("dry-run-must-not-exist.db");
    let dry_run = nummetria(&[
        "--json",
        "--database",
        dry_run_database.to_str().unwrap(),
        "import",
        fixture("valid-v1.json").to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(dry_run.status.code(), Some(0));
    assert!(dry_run.stderr.is_empty());
    let output: Value = serde_json::from_slice(&dry_run.stdout).unwrap();
    assert_eq!(output["data"]["records_inserted"], Value::Null);
    assert_eq!(output["data"]["records_already_present"], Value::Null);
    assert_eq!(output["warnings"].as_array().unwrap().len(), 1);
    assert!(!dry_run_database.exists());
}

#[test]
fn invalid_inputs_use_exit_two_and_never_open_sqlite() {
    let directory = tempfile::tempdir().unwrap();

    for fixture_name in ["malformed-v1.json", "mixed-invalid-v1.json"] {
        let database = directory.path().join(format!("{fixture_name}.db"));
        let output = nummetria(&[
            "--json",
            "--database",
            database.to_str().unwrap(),
            "import",
            fixture(fixture_name).to_str().unwrap(),
        ]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["error"]["code"], "invalid_import");
        assert!(!database.exists());
    }

    let unsupported = directory.path().join("unsupported.json");
    fs::write(&unsupported, r#"{"format_version":2,"records":[]}"#).unwrap();
    let database = directory.path().join("unsupported.db");
    let output = import(&database, &unsupported);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("unsupported usage exchange format version 2"));
    assert!(!database.exists());

    let missing_database = nummetria(&["import", fixture("valid-v1.json").to_str().unwrap()]);
    assert_eq!(missing_database.status.code(), Some(2));
    assert!(text(&missing_database.stderr).contains("--database"));
}

#[test]
fn file_and_storage_failures_use_their_documented_exit_codes() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("usage.db");
    let missing = directory.path().join("missing.json");
    let input_failure = import(&database, &missing);
    assert_eq!(input_failure.status.code(), Some(3));

    let storage_failure = import(directory.path(), &fixture("valid-v1.json"));
    assert_eq!(storage_failure.status.code(), Some(4));
    assert!(text(&storage_failure.stderr).contains("database error"));
}

#[test]
fn a_conflicting_batch_rolls_back_every_new_record() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("usage.db");
    assert_eq!(
        import(&database, &fixture("valid-v1.json")).status.code(),
        Some(0)
    );

    let mut new_record = valid_record();
    new_record["id"] = "new-record-that-must-roll-back".into();
    let mut conflict = valid_record();
    conflict["project"] = "different-project".into();
    let conflict_exchange = directory.path().join("conflict.json");
    write_exchange(&conflict_exchange, vec![new_record, conflict]);

    let conflict = import(&database, &conflict_exchange);
    assert_eq!(conflict.status.code(), Some(4));
    assert!(text(&conflict.stderr).contains("already exists with different contents"));

    let usage = nummetria(&["--json", "--database", database.to_str().unwrap(), "usage"]);
    let usage: Value = serde_json::from_slice(&usage.stdout).unwrap();
    assert_eq!(usage["data"]["records"].as_array().unwrap().len(), 1);
    assert_ne!(
        usage["data"]["records"][0]["id"],
        "new-record-that-must-roll-back"
    );
}

#[test]
fn json_round_trip_preserves_every_record() {
    let directory = tempfile::tempdir().unwrap();
    let source_database = directory.path().join("source.db");
    assert_eq!(
        import(&source_database, &fixture("valid-v1.json"))
            .status
            .code(),
        Some(0)
    );

    let exported = nummetria(&[
        "--quiet",
        "--database",
        source_database.to_str().unwrap(),
        "export",
        "--format",
        "json",
    ]);
    assert_eq!(exported.status.code(), Some(0));
    assert!(!exported.stdout.is_empty());

    let exchange_path = directory.path().join("round trip.json");
    fs::write(&exchange_path, &exported.stdout).unwrap();
    let target_database = directory.path().join("target.db");
    assert_eq!(
        import(&target_database, &exchange_path).status.code(),
        Some(0)
    );

    let reexported = nummetria(&[
        "--database",
        target_database.to_str().unwrap(),
        "export",
        "--format",
        "json",
    ]);
    let original: Value = serde_json::from_slice(&exported.stdout).unwrap();
    let round_tripped: Value = serde_json::from_slice(&reexported.stdout).unwrap();
    assert_eq!(round_tripped, original);
}

#[test]
fn csv_escapes_text_sums_quantities_and_never_overwrites() {
    let directory = tempfile::tempdir().unwrap();
    let mut record = valid_record();
    record["project"] = "project, \"quoted\"".into();
    record["quantities"] = json!([
        {"kind": "requests", "amount": "1.25"},
        {"kind": "requests", "amount": "2.75"}
    ]);
    let exchange = directory.path().join("quoted.json");
    write_exchange(&exchange, vec![record]);
    let database = directory.path().join("usage.db");
    assert_eq!(import(&database, &exchange).status.code(), Some(0));

    let csv_output = nummetria(&[
        "--quiet",
        "--database",
        database.to_str().unwrap(),
        "export",
        "--format",
        "csv",
    ]);
    assert_eq!(csv_output.status.code(), Some(0));
    let mut reader = csv::Reader::from_reader(csv_output.stdout.as_slice());
    let headers = reader.headers().unwrap().clone();
    let row = reader.records().next().unwrap().unwrap();
    let project = headers
        .iter()
        .position(|header| header == "project")
        .unwrap();
    let requests = headers
        .iter()
        .position(|header| header == "requests")
        .unwrap();
    assert_eq!(&row[project], "project, \"quoted\"");
    assert_eq!(&row[requests], "4.00");
    assert!(reader.records().next().is_none());

    let destination = directory.path().join("existing.csv");
    fs::write(&destination, "original").unwrap();
    let refused = nummetria(&[
        "--database",
        database.to_str().unwrap(),
        "export",
        "--format",
        "csv",
        "--output",
        destination.to_str().unwrap(),
    ]);
    assert_eq!(refused.status.code(), Some(3));
    assert_eq!(fs::read_to_string(destination).unwrap(), "original");
}
