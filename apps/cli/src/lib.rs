use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use nummetria_core::{
    Cost, CostEvidence, ExchangeError, RecordValidationError, UsageExchange, UsageKind, UsageRecord,
};
use nummetria_storage::{SqliteStorage, UsageAggregate, UsageQuery};
use serde::Serialize;

const OUTPUT_VERSION: u16 = 1;
const EXIT_INVALID_INPUT: u8 = 2;
const EXIT_FILE_IO: u8 = 3;
const EXIT_STORAGE: u8 = 4;

/// Nummetria's command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "nummetria",
    version,
    about = "Understand your AI usage, costs, and budgets",
    long_about = "Nummetria is a local-first tool for collecting and reporting AI usage across providers.",
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOptions,

    #[command(subcommand)]
    pub command: Command,
}

/// Options accepted by every Nummetria command.
#[derive(Debug, clap::Args)]
pub struct GlobalOptions {
    /// Emit versioned machine-readable JSON.
    #[arg(long, global = true)]
    pub json: bool,

    /// Suppress non-essential output.
    #[arg(long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Disable ANSI color output.
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Use an explicit TOML configuration file.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Use an explicit SQLite database file.
    #[arg(long, global = true, value_name = "PATH")]
    pub database: Option<PathBuf>,

    /// Increase diagnostic output. Repeat for more detail.
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Debug, clap::Args)]
pub struct ImportArgs {
    /// Versioned Nummetria usage exchange file.
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Validate without opening or changing a database.
    #[arg(long)]
    pub dry_run: bool,
}

/// Public v0.1 command groups.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize local paths and provider configuration.
    Setup,
    /// Show a concise usage and cost summary.
    Status,
    /// Collect new usage from configured providers.
    Collect,
    /// Query and group stored usage.
    Usage,
    /// Manage and test provider connections.
    Providers,
    /// Create and check local budgets.
    Budget,
    /// Validate and store an exchange file.
    Import(ImportArgs),
    /// Export normalized usage as JSON or CSV.
    Export,
    /// Inspect and change non-secret configuration.
    Config,
    /// Inspect paths, back up, or deliberately delete local data.
    Data,
    /// Diagnose installation, storage, and provider health.
    Doctor,
    /// Generate a shell completion script.
    Completion,
    /// Print version and build information.
    Version,
}

#[derive(Debug, Serialize)]
struct OutputEnvelope<T> {
    output_version: u16,
    command: &'static str,
    data: T,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    output_version: u16,
    command: &'static str,
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    details: Vec<RecordValidationError>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct ImportSummary {
    records_read: usize,
    records_valid: usize,
    records_inserted: Option<usize>,
    records_already_present: Option<usize>,
    dry_run: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct QuantitySummary {
    kind: &'static str,
    amount: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct CostSummary {
    evidence: &'static str,
    currency: String,
    amount: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct StatusSummary {
    record_count: usize,
    quantities: Vec<QuantitySummary>,
    costs: Vec<CostSummary>,
    unknown_cost_record_count: usize,
}

#[derive(Debug, Serialize)]
struct UsageOutput<'a> {
    records: &'a [UsageRecord],
}

struct CliFailure {
    exit_code: u8,
    code: &'static str,
    message: String,
    details: Vec<RecordValidationError>,
}

impl CliFailure {
    fn new(exit_code: u8, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            code,
            message: message.into(),
            details: Vec::new(),
        }
    }
}

/// Parse process arguments and run the selected command.
pub fn run() -> ExitCode {
    let cli = Cli::parse();
    let stdout = io::stdout();
    let stderr = io::stderr();
    run_with_io(cli, &mut stdout.lock(), &mut stderr.lock())
}

fn run_with_io(cli: Cli, stdout: &mut dyn Write, stderr: &mut dyn Write) -> ExitCode {
    let command = command_name(&cli.command);
    let result = match &cli.command {
        Command::Version => writeln!(
            stdout,
            "{} {}",
            nummetria_core::PRODUCT_NAME,
            env!("CARGO_PKG_VERSION")
        )
        .map_err(output_failure),
        Command::Import(args) => run_import(&cli.global, args, stdout, stderr),
        Command::Status => run_status(&cli.global, stdout),
        Command::Usage => run_usage(&cli.global, stdout),
        _ => Err(CliFailure::new(
            EXIT_INVALID_INPUT,
            "not_implemented",
            format!("the '{command}' command is part of the v0.1 contract but is not implemented"),
        )),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            render_failure(&cli.global, command, &failure, stderr);
            ExitCode::from(failure.exit_code)
        }
    }
}

fn run_status(global: &GlobalOptions, stdout: &mut dyn Write) -> Result<(), CliFailure> {
    let storage = open_database(global, "status")?;
    let aggregate = storage
        .aggregate_usage(&UsageQuery::default())
        .map_err(storage_failure)?;
    let summary = status_summary(aggregate);

    if global.json {
        render_json_success("status", summary, Vec::new(), stdout)
    } else {
        writeln!(stdout, "Records: {}", summary.record_count).map_err(output_failure)?;
        writeln!(stdout, "Quantities:").map_err(output_failure)?;
        if summary.quantities.is_empty() {
            writeln!(stdout, "  none").map_err(output_failure)?;
        } else {
            for quantity in &summary.quantities {
                writeln!(stdout, "  {}: {}", quantity.kind, quantity.amount)
                    .map_err(output_failure)?;
            }
        }
        writeln!(stdout, "Costs:").map_err(output_failure)?;
        if summary.costs.is_empty() {
            writeln!(stdout, "  none").map_err(output_failure)?;
        } else {
            for cost in &summary.costs {
                writeln!(
                    stdout,
                    "  {} {}: {}",
                    cost.evidence, cost.currency, cost.amount
                )
                .map_err(output_failure)?;
            }
        }
        writeln!(
            stdout,
            "Unknown-cost records: {}",
            summary.unknown_cost_record_count
        )
        .map_err(output_failure)
    }
}

fn run_usage(global: &GlobalOptions, stdout: &mut dyn Write) -> Result<(), CliFailure> {
    let storage = open_database(global, "usage")?;
    let records = storage
        .query_usage(&UsageQuery::default())
        .map_err(storage_failure)?;

    if global.json {
        render_json_success(
            "usage",
            UsageOutput { records: &records },
            Vec::new(),
            stdout,
        )
    } else if records.is_empty() {
        writeln!(stdout, "No usage records found.").map_err(output_failure)
    } else {
        for record in &records {
            writeln!(
                stdout,
                "{}..{}  {}  model={}  project={}  id={}",
                record.time_range.start.to_rfc3339(),
                record.time_range.end.to_rfc3339(),
                record.provider.as_str(),
                record.model.as_ref().map_or("-", |value| value.as_str()),
                record.project.as_ref().map_or("-", |value| value.as_str()),
                record.id.as_str(),
            )
            .map_err(output_failure)?;
            let quantities = record
                .quantities
                .iter()
                .map(|quantity| format!("{}={}", usage_kind_name(quantity.kind), quantity.amount))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(stdout, "  usage: {quantities}").map_err(output_failure)?;
            writeln!(stdout, "  cost: {}", human_cost(&record.cost)).map_err(output_failure)?;
        }
        Ok(())
    }
}

fn open_database(
    global: &GlobalOptions,
    command: &'static str,
) -> Result<SqliteStorage, CliFailure> {
    let database = global.database.as_ref().ok_or_else(|| {
        CliFailure::new(
            EXIT_INVALID_INPUT,
            "database_required",
            format!("--database <PATH> is required for the {command} command"),
        )
    })?;
    SqliteStorage::open(database).map_err(storage_failure)
}

fn status_summary(aggregate: UsageAggregate) -> StatusSummary {
    StatusSummary {
        record_count: aggregate.record_count,
        quantities: aggregate
            .quantities
            .into_iter()
            .map(|quantity| QuantitySummary {
                kind: usage_kind_name(quantity.kind),
                amount: quantity.amount.to_string(),
            })
            .collect(),
        costs: aggregate
            .costs
            .into_iter()
            .map(|cost| CostSummary {
                evidence: cost_evidence_name(&cost.evidence),
                currency: cost.currency.as_str().to_owned(),
                amount: cost.amount.to_string(),
            })
            .collect(),
        unknown_cost_record_count: aggregate.unknown_cost_record_count,
    }
}

fn human_cost(cost: &Cost) -> String {
    match cost {
        Cost::Reported { amount, currency } => {
            format!("reported {} {amount}", currency.as_str())
        }
        Cost::Calculated {
            amount, currency, ..
        } => format!("calculated {} {amount}", currency.as_str()),
        Cost::Estimated {
            amount, currency, ..
        } => format!("estimated {} {amount}", currency.as_str()),
        Cost::Unknown => "unknown".to_owned(),
    }
}

fn cost_evidence_name(evidence: &CostEvidence) -> &'static str {
    match evidence {
        CostEvidence::Reported => "reported",
        CostEvidence::Calculated => "calculated",
        CostEvidence::Estimated => "estimated",
        CostEvidence::Unknown => "unknown",
    }
}

fn usage_kind_name(kind: UsageKind) -> &'static str {
    match kind {
        UsageKind::InputTokens => "input_tokens",
        UsageKind::OutputTokens => "output_tokens",
        UsageKind::CachedTokens => "cached_tokens",
        UsageKind::CacheWriteTokens => "cache_write_tokens",
        UsageKind::ReasoningTokens => "reasoning_tokens",
        UsageKind::Requests => "requests",
        UsageKind::Images => "images",
        UsageKind::AudioSeconds => "audio_seconds",
        UsageKind::VideoSeconds => "video_seconds",
        UsageKind::ToolCalls => "tool_calls",
        UsageKind::WebSearches => "web_searches",
        UsageKind::ComputeSeconds => "compute_seconds",
    }
}

fn run_import(
    global: &GlobalOptions,
    args: &ImportArgs,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), CliFailure> {
    let input = std::fs::read_to_string(&args.file).map_err(|error| {
        CliFailure::new(
            EXIT_FILE_IO,
            "input_io",
            format!("could not read {}: {error}", args.file.display()),
        )
    })?;
    let exchange = UsageExchange::from_json_str(&input).map_err(exchange_failure)?;
    let record_count = exchange.records.len();

    if args.dry_run {
        let warning =
            "dry run did not open SQLite; stored duplicates and conflicts were not checked";
        let summary = ImportSummary {
            records_read: record_count,
            records_valid: record_count,
            records_inserted: None,
            records_already_present: None,
            dry_run: true,
        };
        if global.json {
            render_json_success("import", summary, vec![warning.to_owned()], stdout)?;
        } else {
            if !global.quiet {
                writeln!(
                    stdout,
                    "Validated {record_count} record(s); no database was opened."
                )
                .map_err(output_failure)?;
            }
            writeln!(stderr, "Warning: {warning}.").map_err(output_failure)?;
        }
        return Ok(());
    }

    let database = global.database.as_ref().ok_or_else(|| {
        CliFailure::new(
            EXIT_INVALID_INPUT,
            "database_required",
            "--database <PATH> is required unless import is run with --dry-run",
        )
    })?;
    let mut storage = SqliteStorage::open(database).map_err(storage_failure)?;
    let inserted = storage
        .insert_usage_records(&exchange.records)
        .map_err(storage_failure)?;
    let summary = ImportSummary {
        records_read: record_count,
        records_valid: record_count,
        records_inserted: Some(inserted.inserted),
        records_already_present: Some(inserted.already_present),
        dry_run: false,
    };

    if global.json {
        render_json_success("import", summary, Vec::new(), stdout)
    } else if global.quiet {
        Ok(())
    } else {
        writeln!(
            stdout,
            "Imported {record_count} record(s): {} inserted, {} already present.",
            inserted.inserted, inserted.already_present
        )
        .map_err(output_failure)
    }
}

fn exchange_failure(error: ExchangeError) -> CliFailure {
    match error {
        ExchangeError::InvalidRecords(details) => CliFailure {
            exit_code: EXIT_INVALID_INPUT,
            code: "invalid_import",
            message: format!("{} usage record(s) failed validation", details.len()),
            details,
        },
        other => CliFailure::new(EXIT_INVALID_INPUT, "invalid_import", other.to_string()),
    }
}

fn storage_failure(error: nummetria_storage::StorageError) -> CliFailure {
    CliFailure::new(EXIT_STORAGE, "storage_failure", error.to_string())
}

fn output_failure(error: io::Error) -> CliFailure {
    CliFailure::new(
        EXIT_FILE_IO,
        "output_io",
        format!("could not write output: {error}"),
    )
}

fn render_json_success<T: Serialize>(
    command: &'static str,
    data: T,
    warnings: Vec<String>,
    stdout: &mut dyn Write,
) -> Result<(), CliFailure> {
    serde_json::to_writer_pretty(
        &mut *stdout,
        &OutputEnvelope {
            output_version: OUTPUT_VERSION,
            command,
            data,
            warnings,
        },
    )
    .map_err(|error| output_failure(io::Error::other(error)))?;
    writeln!(stdout).map_err(output_failure)
}

fn render_failure(
    global: &GlobalOptions,
    command: &'static str,
    failure: &CliFailure,
    stderr: &mut dyn Write,
) {
    if global.json {
        let _ = serde_json::to_writer_pretty(
            &mut *stderr,
            &ErrorEnvelope {
                output_version: OUTPUT_VERSION,
                command,
                error: ErrorBody {
                    code: failure.code,
                    message: failure.message.clone(),
                    details: failure.details.clone(),
                },
            },
        );
        let _ = writeln!(stderr);
    } else {
        let _ = writeln!(stderr, "Error: {}", failure.message);
        for detail in &failure.details {
            let _ = writeln!(stderr, "  {}: {}", detail.location, detail.message);
        }
    }
}

fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Setup => "setup",
        Command::Status => "status",
        Command::Collect => "collect",
        Command::Usage => "usage",
        Command::Providers => "providers",
        Command::Budget => "budget",
        Command::Import(_) => "import",
        Command::Export => "export",
        Command::Config => "config",
        Command::Data => "data",
        Command::Doctor => "doctor",
        Command::Completion => "completion",
        Command::Version => "version",
    }
}

/// Build the Clap command definition for documentation and tests.
pub fn command() -> clap::Command {
    Cli::command()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    fn run(args: &[&str]) -> (bool, String, String) {
        let cli = Cli::try_parse_from(args).unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = run_with_io(cli, &mut stdout, &mut stderr);
        (
            exit == ExitCode::SUCCESS,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    #[test]
    fn command_definition_is_valid() {
        super::command().debug_assert();
    }

    #[test]
    fn parses_every_public_command() {
        let command_lines: &[&[&str]] = &[
            &["nummetria", "setup"],
            &["nummetria", "status"],
            &["nummetria", "collect"],
            &["nummetria", "usage"],
            &["nummetria", "providers"],
            &["nummetria", "budget"],
            &["nummetria", "import", "usage.json"],
            &["nummetria", "export"],
            &["nummetria", "config"],
            &["nummetria", "data"],
            &["nummetria", "doctor"],
            &["nummetria", "completion"],
            &["nummetria", "version"],
        ];

        for args in command_lines {
            Cli::try_parse_from(*args)
                .unwrap_or_else(|error| panic!("failed to parse {args:?}: {error}"));
        }
    }

    #[test]
    fn parses_global_options_before_or_after_a_command() {
        let before = Cli::try_parse_from(["nummetria", "--json", "status"])
            .expect("global option before command");
        let after = Cli::try_parse_from(["nummetria", "status", "--json"])
            .expect("global option after command");

        assert!(before.global.json);
        assert!(after.global.json);
    }

    #[test]
    fn dry_run_validates_without_a_database() {
        let (success, stdout, stderr) = run(&[
            "nummetria",
            "import",
            "../../fixtures/exchange/valid-v1.json",
            "--dry-run",
        ]);
        assert!(success);
        assert!(stdout.contains("Validated 1 record(s)"));
        assert!(stderr.contains("stored duplicates and conflicts were not checked"));
    }

    #[test]
    fn import_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("usage.db");
        let database = database.to_str().unwrap();
        let args = [
            "nummetria",
            "--database",
            database,
            "import",
            "../../fixtures/exchange/valid-v1.json",
        ];

        let (first_success, first, _) = run(&args);
        let (second_success, second, _) = run(&args);

        assert!(first_success);
        assert!(second_success);
        assert!(first.contains("1 inserted, 0 already present"));
        assert!(second.contains("0 inserted, 1 already present"));
    }

    #[test]
    fn invalid_records_are_reported_together_without_opening_sqlite() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("must-not-exist.db");
        let database_string = database.to_str().unwrap();
        let (success, _, stderr) = run(&[
            "nummetria",
            "--database",
            database_string,
            "import",
            "../../fixtures/exchange/mixed-invalid-v1.json",
        ]);

        assert!(!success);
        assert!(stderr.contains("records[1]"));
        assert!(!database.exists());
    }

    #[test]
    fn status_and_usage_read_back_imported_records() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("usage.db");
        let database = database.to_str().unwrap();
        let import_args = [
            "nummetria",
            "--database",
            database,
            "import",
            "../../fixtures/exchange/valid-v1.json",
        ];
        assert!(run(&import_args).0);

        let (status_success, status, status_error) =
            run(&["nummetria", "--database", database, "--json", "status"]);
        assert!(status_success, "{status_error}");
        let status: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(status["data"]["record_count"], 1);
        assert_eq!(status["data"]["quantities"][0]["amount"], "1250");
        assert_eq!(status["data"]["costs"][0]["evidence"], "reported");

        let (usage_success, usage, usage_error) =
            run(&["nummetria", "--database", database, "--json", "usage"]);
        assert!(usage_success, "{usage_error}");
        let usage: serde_json::Value = serde_json::from_str(&usage).unwrap();
        assert_eq!(
            usage["data"]["records"][0]["id"],
            "openai:usage:2026-08-17:project-a"
        );
    }
}
