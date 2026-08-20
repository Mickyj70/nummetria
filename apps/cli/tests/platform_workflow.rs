use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/exchange")
        .join(name)
}

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nummetria"));
    command
        .env_remove("NUMMETRIA_CONFIG")
        .env_remove("NUMMETRIA_DATABASE");
    command
}

fn run(args: &[&str]) -> Output {
    command().args(args).output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn configuration_database_precedence_is_visible_and_predictable() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("settings/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(
        &config,
        "config_version = 1\ndatabase_path = 'configured.db'\n",
    )
    .unwrap();

    let output = run(&[
        "--json",
        "--config",
        config.to_str().unwrap(),
        "config",
        "show",
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["data"]["database_source"], "configuration");
    assert_eq!(
        body["data"]["database_path"],
        config
            .parent()
            .unwrap()
            .join("configured.db")
            .display()
            .to_string()
    );

    let environment_database = directory.path().join("environment.db");
    let output = command()
        .env("NUMMETRIA_CONFIG", &config)
        .env("NUMMETRIA_DATABASE", &environment_database)
        .args(["--json", "data", "path"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["data"]["source"], "environment");
    assert_eq!(
        body["data"]["path"],
        environment_database.display().to_string()
    );

    let command_database = directory.path().join("command.db");
    let output = command()
        .env("NUMMETRIA_CONFIG", &config)
        .env("NUMMETRIA_DATABASE", &environment_database)
        .args([
            "--json",
            "--database",
            command_database.to_str().unwrap(),
            "data",
            "path",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["data"]["source"], "command_line");
    assert_eq!(body["data"]["path"], command_database.display().to_string());
}

#[test]
fn configured_database_supports_import_backup_and_deletion() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    let database = directory.path().join("data/usage.db");
    fs::write(
        &config,
        format!(
            "config_version = 1\ndatabase_path = {:?}\n",
            database.display().to_string()
        ),
    )
    .unwrap();
    let config_arg = config.to_str().unwrap();

    let imported = run(&[
        "--config",
        config_arg,
        "import",
        fixture("valid-v1.json").to_str().unwrap(),
    ]);
    assert_eq!(
        imported.status.code(),
        Some(0),
        "{}",
        text(&imported.stderr)
    );
    assert!(database.exists());

    let backup = directory.path().join("backup.db");
    let backup_arg = backup.to_str().unwrap();
    let backed_up = run(&[
        "--config", config_arg, "data", "backup", "--output", backup_arg,
    ]);
    assert_eq!(
        backed_up.status.code(),
        Some(0),
        "{}",
        text(&backed_up.stderr)
    );
    assert!(backup.exists());

    let refused = run(&[
        "--config", config_arg, "data", "backup", "--output", backup_arg,
    ]);
    assert_eq!(refused.status.code(), Some(3));

    let missing_scope = run(&["--config", config_arg, "data", "delete", "--yes"]);
    assert_eq!(missing_scope.status.code(), Some(2));

    let deleted = run(&[
        "--json", "--config", config_arg, "data", "delete", "--all", "--yes",
    ]);
    assert_eq!(deleted.status.code(), Some(0), "{}", text(&deleted.stderr));
    let body: Value = serde_json::from_slice(&deleted.stdout).unwrap();
    assert_eq!(body["data"]["records_deleted"], 1);
    assert!(config.exists());
    assert!(database.exists());

    let status = run(&["--json", "--config", config_arg, "status"]);
    assert_eq!(status.status.code(), Some(0), "{}", text(&status.stderr));
    let body: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(body["data"]["record_count"], 0);
}

#[test]
fn invalid_configuration_is_safe_and_never_echoes_secret_values() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    let secret = "sk-test-super-secret-value";
    fs::write(
        &config,
        format!("config_version = 1\napi_key = '{secret}'\n"),
    )
    .unwrap();

    let output = run(&[
        "--json",
        "--config",
        config.to_str().unwrap(),
        "config",
        "validate",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!text(&output.stderr).contains(secret));
    let body: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(body["error"]["code"], "invalid_configuration");

    let empty_environment = command()
        .env("NUMMETRIA_CONFIG", "")
        .args(["config", "validate"])
        .output()
        .unwrap();
    assert_eq!(empty_environment.status.code(), Some(2));
    assert!(text(&empty_environment.stderr).contains("cannot be empty"));

    let dry_run = command()
        .env("NUMMETRIA_CONFIG", "")
        .args([
            "import",
            fixture("valid-v1.json").to_str().unwrap(),
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert_eq!(dry_run.status.code(), Some(0), "{}", text(&dry_run.stderr));
}
