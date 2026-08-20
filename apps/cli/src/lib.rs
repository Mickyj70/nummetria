use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use nummetria_core::{
    CollectionSource, Cost, CostEvidence, ExchangeError, RecordValidationError, UsageExchange,
    UsageKind, UsageRecord,
};
use nummetria_platform::{
    ConfigError, ConfigSource, EnvironmentOverrides, PlatformPaths, ResolveOptions, ResolvedConfig,
    SetupOutcome, resolve_config, write_initial_config,
};
use nummetria_storage::{SqliteStorage, UsageAggregate, UsageQuery};
use rust_decimal::Decimal;
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

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    Json,
    Csv,
}

impl ExportFormat {
    fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }
}

#[derive(Debug, clap::Args)]
pub struct ExportArgs {
    /// Output representation.
    #[arg(long, value_enum)]
    pub format: ExportFormat,

    /// Write to a new file instead of standard output.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the selected configuration file path.
    Path,
    /// Show resolved non-secret configuration and its sources.
    Show,
    /// Validate the selected configuration without changing it.
    Validate,
}

#[derive(Debug, clap::Args)]
pub struct DataArgs {
    #[command(subcommand)]
    pub command: DataCommand,
}

#[derive(Debug, Subcommand)]
pub enum DataCommand {
    /// Print the resolved SQLite database path.
    Path,
    /// Copy the SQLite database to a new backup file.
    Backup {
        /// Destination path, which must not already exist.
        #[arg(long, value_name = "PATH")]
        output: PathBuf,
    },
    /// Deliberately delete all locally stored usage records.
    Delete {
        /// Confirm that all usage data is in scope.
        #[arg(long)]
        all: bool,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
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
    Export(ExportArgs),
    /// Inspect and change non-secret configuration.
    Config(ConfigArgs),
    /// Inspect paths, back up, or deliberately delete local data.
    Data(DataArgs),
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

#[derive(Debug, Serialize, PartialEq, Eq)]
struct ExportSummary {
    records_exported: usize,
    format: &'static str,
    output: String,
}

#[derive(Debug, Serialize)]
struct SetupSummary {
    config_path: String,
    data_directory: String,
    database_path: String,
    config_created: bool,
}

#[derive(Debug, Serialize)]
struct ConfigPathSummary {
    path: String,
    source: ConfigSource,
    exists: bool,
}

#[derive(Debug, Serialize)]
struct ConfigShowSummary {
    config: ConfigPathSummary,
    database_path: String,
    database_source: ConfigSource,
}

#[derive(Debug, Serialize)]
struct DataPathSummary {
    path: String,
    source: ConfigSource,
}

struct RuntimeContext {
    paths: PlatformPaths,
    environment: EnvironmentOverrides,
}

#[derive(Debug, Serialize)]
struct CsvRecord {
    schema_version: u16,
    id: String,
    provider: String,
    model: String,
    project: String,
    period_start: String,
    period_end: String,
    collected_at: String,
    cost_evidence: &'static str,
    cost_amount: String,
    cost_currency: String,
    pricing_reference: String,
    source_kind: &'static str,
    source_operation: String,
    source_format: String,
    source_name: String,
    input_tokens: String,
    output_tokens: String,
    cached_tokens: String,
    cache_write_tokens: String,
    reasoning_tokens: String,
    requests: String,
    images: String,
    audio_seconds: String,
    video_seconds: String,
    tool_calls: String,
    web_searches: String,
    compute_seconds: String,
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
    let command = command_name(&cli.command);
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();
    let paths = match PlatformPaths::discover() {
        Ok(paths) => paths,
        Err(error) => {
            let failure = configuration_failure(ConfigError::Paths(error));
            render_failure(&cli.global, command, &failure, &mut stderr);
            return ExitCode::from(failure.exit_code);
        }
    };
    let environment = match EnvironmentOverrides::from_process() {
        Ok(environment) => environment,
        Err(error) => {
            let failure = configuration_failure(error);
            render_failure(&cli.global, command, &failure, &mut stderr);
            return ExitCode::from(failure.exit_code);
        }
    };
    let context = RuntimeContext { paths, environment };
    run_with_io(
        cli,
        &context,
        &mut io::stdin().lock(),
        &mut stdout,
        &mut stderr,
    )
}

fn run_with_io(
    cli: Cli,
    context: &RuntimeContext,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> ExitCode {
    let command = command_name(&cli.command);
    let result = match &cli.command {
        Command::Version => writeln!(
            stdout,
            "{} {}",
            nummetria_core::PRODUCT_NAME,
            env!("CARGO_PKG_VERSION")
        )
        .map_err(output_failure),
        Command::Setup => run_setup(&cli.global, context, stdout),
        Command::Config(args) => run_config(&cli.global, args, context, stdout),
        Command::Data(args) => run_data(&cli.global, args, context, stdin, stdout),
        Command::Import(args) => run_import(&cli.global, args, context, stdout, stderr),
        Command::Status => run_status(&cli.global, context, stdout),
        Command::Usage => run_usage(&cli.global, context, stdout),
        Command::Export(args) => run_export(&cli.global, args, context, stdout),
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

fn run_export(
    global: &GlobalOptions,
    args: &ExportArgs,
    context: &RuntimeContext,
    stdout: &mut dyn Write,
) -> Result<(), CliFailure> {
    let storage = open_database(global, context)?;
    let records = storage
        .query_usage(&UsageQuery::default())
        .map_err(storage_failure)?;
    let bytes = match args.format {
        ExportFormat::Json => {
            let mut bytes = serde_json::to_vec_pretty(&UsageExchange::new(records.clone()))
                .map_err(serialization_failure)?;
            bytes.push(b'\n');
            bytes
        }
        ExportFormat::Csv => render_csv(&records)?,
    };

    if let Some(path) = &args.output {
        write_new_file(path, &bytes)?;
        let summary = ExportSummary {
            records_exported: records.len(),
            format: args.format.name(),
            output: path.display().to_string(),
        };
        if global.json {
            render_json_success("export", summary, Vec::new(), stdout)
        } else if global.quiet {
            Ok(())
        } else {
            writeln!(
                stdout,
                "Exported {} record(s) as {} to {}.",
                records.len(),
                args.format.name(),
                path.display()
            )
            .map_err(output_failure)
        }
    } else {
        stdout.write_all(&bytes).map_err(output_failure)
    }
}

fn run_setup(
    global: &GlobalOptions,
    context: &RuntimeContext,
    stdout: &mut dyn Write,
) -> Result<(), CliFailure> {
    let outcome = write_initial_config(&context.paths).map_err(configuration_failure)?;
    let summary = SetupSummary {
        config_path: context.paths.config_file().display().to_string(),
        data_directory: context.paths.data_dir().display().to_string(),
        database_path: context.paths.database_file().display().to_string(),
        config_created: outcome == SetupOutcome::Created,
    };

    if global.json {
        render_json_success("setup", summary, Vec::new(), stdout)
    } else if global.quiet {
        Ok(())
    } else {
        let action = match outcome {
            SetupOutcome::Created => "Created",
            SetupOutcome::AlreadyPresent => "Kept existing",
        };
        writeln!(stdout, "{action} configuration: {}", summary.config_path)
            .and_then(|()| writeln!(stdout, "Data directory: {}", summary.data_directory))
            .and_then(|()| writeln!(stdout, "Database: {}", summary.database_path))
            .map_err(output_failure)
    }
}

fn run_config(
    global: &GlobalOptions,
    args: &ConfigArgs,
    context: &RuntimeContext,
    stdout: &mut dyn Write,
) -> Result<(), CliFailure> {
    let resolved = resolve(global, context)?;
    match args.command {
        ConfigCommand::Path => {
            let summary = ConfigPathSummary {
                path: resolved.config_path.display().to_string(),
                source: resolved.config_source,
                exists: resolved.config_exists,
            };
            if global.json {
                render_json_success("config", summary, Vec::new(), stdout)
            } else {
                writeln!(stdout, "{}", summary.path).map_err(output_failure)
            }
        }
        ConfigCommand::Show => {
            let summary = config_show_summary(&resolved);
            if global.json {
                render_json_success("config", summary, Vec::new(), stdout)
            } else {
                writeln!(
                    stdout,
                    "Configuration: {} (source: {}, exists: {})",
                    summary.config.path,
                    source_name(summary.config.source),
                    summary.config.exists
                )
                .and_then(|()| {
                    writeln!(
                        stdout,
                        "Database: {} (source: {})",
                        summary.database_path,
                        source_name(summary.database_source)
                    )
                })
                .map_err(output_failure)
            }
        }
        ConfigCommand::Validate => {
            if global.json {
                render_json_success("config", config_show_summary(&resolved), Vec::new(), stdout)
            } else if global.quiet {
                Ok(())
            } else {
                writeln!(
                    stdout,
                    "Configuration is valid: {}",
                    resolved.config_path.display()
                )
                .map_err(output_failure)
            }
        }
    }
}

fn run_data(
    global: &GlobalOptions,
    args: &DataArgs,
    context: &RuntimeContext,
    _stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
) -> Result<(), CliFailure> {
    match &args.command {
        DataCommand::Path => {
            let resolved = resolve(global, context)?;
            let summary = DataPathSummary {
                path: resolved.database_path.display().to_string(),
                source: resolved.database_source,
            };
            if global.json {
                render_json_success("data", summary, Vec::new(), stdout)
            } else {
                writeln!(stdout, "{}", summary.path).map_err(output_failure)
            }
        }
        DataCommand::Backup { .. } | DataCommand::Delete { .. } => Err(CliFailure::new(
            EXIT_INVALID_INPUT,
            "not_implemented",
            "this data operation is not implemented yet",
        )),
    }
}

fn config_show_summary(resolved: &ResolvedConfig) -> ConfigShowSummary {
    ConfigShowSummary {
        config: ConfigPathSummary {
            path: resolved.config_path.display().to_string(),
            source: resolved.config_source,
            exists: resolved.config_exists,
        },
        database_path: resolved.database_path.display().to_string(),
        database_source: resolved.database_source,
    }
}

fn source_name(source: ConfigSource) -> &'static str {
    match source {
        ConfigSource::CommandLine => "command line",
        ConfigSource::Environment => "environment",
        ConfigSource::Configuration => "configuration",
        ConfigSource::Default => "default",
    }
}

fn render_csv(records: &[UsageRecord]) -> Result<Vec<u8>, CliFailure> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    for record in records {
        writer
            .serialize(csv_record(record))
            .map_err(|error| serialization_failure(error.to_string()))?;
    }
    writer.flush().map_err(output_failure)?;
    writer
        .into_inner()
        .map_err(|error| output_failure(error.into_error()))
}

fn csv_record(record: &UsageRecord) -> CsvRecord {
    let (cost_evidence, cost_amount, cost_currency, pricing_reference) = match &record.cost {
        Cost::Reported { amount, currency } => (
            "reported",
            amount.to_string(),
            currency.as_str().to_owned(),
            String::new(),
        ),
        Cost::Calculated {
            amount,
            currency,
            pricing_reference,
        } => (
            "calculated",
            amount.to_string(),
            currency.as_str().to_owned(),
            pricing_reference.clone(),
        ),
        Cost::Estimated {
            amount,
            currency,
            pricing_reference,
        } => (
            "estimated",
            amount.to_string(),
            currency.as_str().to_owned(),
            pricing_reference.clone(),
        ),
        Cost::Unknown => ("unknown", String::new(), String::new(), String::new()),
    };
    let (source_kind, source_operation, source_format, source_name) = match &record.source {
        CollectionSource::ProviderApi { operation } => (
            "provider_api",
            operation.clone(),
            String::new(),
            String::new(),
        ),
        CollectionSource::Import {
            format,
            source_name,
        } => ("import", String::new(), format.clone(), source_name.clone()),
    };

    CsvRecord {
        schema_version: record.schema_version,
        id: record.id.as_str().to_owned(),
        provider: record.provider.as_str().to_owned(),
        model: record
            .model
            .as_ref()
            .map_or_else(String::new, |value| value.as_str().to_owned()),
        project: record
            .project
            .as_ref()
            .map_or_else(String::new, |value| value.as_str().to_owned()),
        period_start: record.time_range.start.to_rfc3339(),
        period_end: record.time_range.end.to_rfc3339(),
        collected_at: record.collected_at.to_rfc3339(),
        cost_evidence,
        cost_amount,
        cost_currency,
        pricing_reference,
        source_kind,
        source_operation,
        source_format,
        source_name,
        input_tokens: quantity_total(record, UsageKind::InputTokens),
        output_tokens: quantity_total(record, UsageKind::OutputTokens),
        cached_tokens: quantity_total(record, UsageKind::CachedTokens),
        cache_write_tokens: quantity_total(record, UsageKind::CacheWriteTokens),
        reasoning_tokens: quantity_total(record, UsageKind::ReasoningTokens),
        requests: quantity_total(record, UsageKind::Requests),
        images: quantity_total(record, UsageKind::Images),
        audio_seconds: quantity_total(record, UsageKind::AudioSeconds),
        video_seconds: quantity_total(record, UsageKind::VideoSeconds),
        tool_calls: quantity_total(record, UsageKind::ToolCalls),
        web_searches: quantity_total(record, UsageKind::WebSearches),
        compute_seconds: quantity_total(record, UsageKind::ComputeSeconds),
    }
}

fn quantity_total(record: &UsageRecord, kind: UsageKind) -> String {
    let matching = record
        .quantities
        .iter()
        .filter(|quantity| quantity.kind == kind)
        .collect::<Vec<_>>();
    if matching.is_empty() {
        String::new()
    } else {
        matching
            .into_iter()
            .fold(Decimal::ZERO, |total, quantity| total + quantity.amount)
            .to_string()
    }
}

fn write_new_file(path: &std::path::Path, bytes: &[u8]) -> Result<(), CliFailure> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            CliFailure::new(
                EXIT_FILE_IO,
                if error.kind() == io::ErrorKind::AlreadyExists {
                    "output_exists"
                } else {
                    "output_io"
                },
                format!("could not create {}: {error}", path.display()),
            )
        })?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.flush()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(output_failure(error));
    }
    Ok(())
}

fn serialization_failure(error: impl std::fmt::Display) -> CliFailure {
    CliFailure::new(
        EXIT_FILE_IO,
        "output_io",
        format!("could not serialize export: {error}"),
    )
}

fn run_status(
    global: &GlobalOptions,
    context: &RuntimeContext,
    stdout: &mut dyn Write,
) -> Result<(), CliFailure> {
    let storage = open_database(global, context)?;
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

fn run_usage(
    global: &GlobalOptions,
    context: &RuntimeContext,
    stdout: &mut dyn Write,
) -> Result<(), CliFailure> {
    let storage = open_database(global, context)?;
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
    context: &RuntimeContext,
) -> Result<SqliteStorage, CliFailure> {
    let resolved = resolve(global, context)?;
    ensure_database_directory(&resolved.database_path)?;
    SqliteStorage::open(&resolved.database_path).map_err(storage_failure)
}

fn resolve(global: &GlobalOptions, context: &RuntimeContext) -> Result<ResolvedConfig, CliFailure> {
    resolve_config(
        &ResolveOptions {
            config_path: global.config.clone(),
            database_path: global.database.clone(),
        },
        &context.environment,
        &context.paths,
    )
    .map_err(configuration_failure)
}

fn ensure_database_directory(path: &std::path::Path) -> Result<(), CliFailure> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(parent).map_err(|error| {
        CliFailure::new(
            EXIT_FILE_IO,
            "database_path_io",
            format!(
                "could not create database directory {}: {error}",
                parent.display()
            ),
        )
    })
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
    context: &RuntimeContext,
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

    let mut storage = open_database(global, context)?;
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

fn configuration_failure(error: ConfigError) -> CliFailure {
    let (exit_code, code) = match error {
        ConfigError::Read { .. }
        | ConfigError::CreateDirectory { .. }
        | ConfigError::Write { .. }
        | ConfigError::Serialize(_)
        | ConfigError::Paths(_) => (EXIT_FILE_IO, "configuration_io"),
        ConfigError::EmptyEnvironment { .. }
        | ConfigError::ExplicitConfigMissing(_)
        | ConfigError::Parse { .. }
        | ConfigError::UnsupportedVersion { .. } => (EXIT_INVALID_INPUT, "invalid_configuration"),
    };
    CliFailure::new(exit_code, code, error.to_string())
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
        Command::Export(_) => "export",
        Command::Config(_) => "config",
        Command::Data(_) => "data",
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
        run_in_context(
            args,
            PlatformPaths::from_directories("unused-config", "unused-data"),
            EnvironmentOverrides::default(),
        )
    }

    fn run_in_context(
        args: &[&str],
        paths: PlatformPaths,
        environment: EnvironmentOverrides,
    ) -> (bool, String, String) {
        let cli = Cli::try_parse_from(args).unwrap();
        let context = RuntimeContext { paths, environment };
        let mut stdin = io::Cursor::new(Vec::<u8>::new());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = run_with_io(cli, &context, &mut stdin, &mut stdout, &mut stderr);
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
            &["nummetria", "export", "--format", "json"],
            &["nummetria", "config", "show"],
            &["nummetria", "data", "path"],
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
    fn setup_is_create_once_and_reports_standard_paths() {
        let directory = tempfile::tempdir().unwrap();
        let paths = PlatformPaths::from_directories(
            directory.path().join("config"),
            directory.path().join("data"),
        );

        let (first_success, first, first_error) = run_in_context(
            &["nummetria", "setup"],
            paths.clone(),
            EnvironmentOverrides::default(),
        );
        assert!(first_success, "{first_error}");
        assert!(first.contains("Created configuration"));
        assert!(paths.config_file().exists());
        assert!(!paths.database_file().exists());

        let (second_success, second, second_error) = run_in_context(
            &["nummetria", "setup"],
            paths,
            EnvironmentOverrides::default(),
        );
        assert!(second_success, "{second_error}");
        assert!(second.contains("Kept existing configuration"));
    }

    #[test]
    fn config_and_data_commands_explain_resolved_sources() {
        let directory = tempfile::tempdir().unwrap();
        let paths = PlatformPaths::from_directories(
            directory.path().join("config"),
            directory.path().join("data"),
        );
        std::fs::create_dir_all(paths.config_dir()).unwrap();
        std::fs::write(
            paths.config_file(),
            "config_version = 1\ndatabase_path = 'from-config.db'\n",
        )
        .unwrap();

        let (success, output, error) = run_in_context(
            &["nummetria", "--json", "config", "show"],
            paths.clone(),
            EnvironmentOverrides::default(),
        );
        assert!(success, "{error}");
        let output: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(output["data"]["database_source"], "configuration");
        assert_eq!(
            output["data"]["database_path"],
            paths
                .config_dir()
                .join("from-config.db")
                .display()
                .to_string()
        );

        let environment_database = directory.path().join("from-environment.db");
        let (success, output, error) = run_in_context(
            &["nummetria", "--json", "data", "path"],
            paths,
            EnvironmentOverrides {
                config_path: None,
                database_path: Some(environment_database.clone()),
            },
        );
        assert!(success, "{error}");
        let output: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(output["data"]["source"], "environment");
        assert_eq!(
            output["data"]["path"],
            environment_database.display().to_string()
        );
    }

    #[test]
    fn data_commands_use_the_default_database_without_an_explicit_option() {
        let directory = tempfile::tempdir().unwrap();
        let paths = PlatformPaths::from_directories(
            directory.path().join("config"),
            directory.path().join("data"),
        );

        let (success, output, error) = run_in_context(
            &[
                "nummetria",
                "import",
                "../../fixtures/exchange/valid-v1.json",
            ],
            paths.clone(),
            EnvironmentOverrides::default(),
        );
        assert!(success, "{error}");
        assert!(output.contains("1 inserted"));
        assert!(paths.database_file().exists());

        let (success, output, error) = run_in_context(
            &["nummetria", "--json", "status"],
            paths,
            EnvironmentOverrides::default(),
        );
        assert!(success, "{error}");
        let output: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(output["data"]["record_count"], 1);
    }

    #[test]
    fn dry_run_does_not_read_an_invalid_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let invalid_config = directory.path().join("invalid.toml");
        std::fs::write(&invalid_config, "this is not toml = [").unwrap();
        let (success, output, error) = run_in_context(
            &[
                "nummetria",
                "--config",
                invalid_config.to_str().unwrap(),
                "import",
                "../../fixtures/exchange/valid-v1.json",
                "--dry-run",
            ],
            PlatformPaths::from_directories(
                directory.path().join("config"),
                directory.path().join("data"),
            ),
            EnvironmentOverrides::default(),
        );
        assert!(success, "{error}");
        assert!(output.contains("Validated 1 record(s)"));
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

    #[test]
    fn json_export_round_trips_through_import() {
        let directory = tempfile::tempdir().unwrap();
        let source_database = directory.path().join("source.db");
        let source_database = source_database.to_str().unwrap();
        assert!(
            run(&[
                "nummetria",
                "--database",
                source_database,
                "import",
                "../../fixtures/exchange/valid-v1.json",
            ])
            .0
        );

        let (export_success, exported, export_error) = run(&[
            "nummetria",
            "--database",
            source_database,
            "export",
            "--format",
            "json",
        ]);
        assert!(export_success, "{export_error}");
        let exchange = UsageExchange::from_json_str(&exported).unwrap();
        assert_eq!(exchange.records.len(), 1);

        let export_path = directory.path().join("export.json");
        std::fs::write(&export_path, exported).unwrap();
        let target_database = directory.path().join("target.db");
        let (import_success, summary, import_error) = run(&[
            "nummetria",
            "--database",
            target_database.to_str().unwrap(),
            "import",
            export_path.to_str().unwrap(),
        ]);
        assert!(import_success, "{import_error}");
        assert!(summary.contains("1 inserted"));
    }

    #[test]
    fn csv_export_has_one_row_per_record_and_refuses_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("usage.db");
        let database = database.to_str().unwrap();
        assert!(
            run(&[
                "nummetria",
                "--database",
                database,
                "import",
                "../../fixtures/exchange/valid-v1.json",
            ])
            .0
        );

        let (success, csv, error) = run(&[
            "nummetria",
            "--database",
            database,
            "export",
            "--format",
            "csv",
        ]);
        assert!(success, "{error}");
        let rows = csv.lines().collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains("input_tokens"));
        assert!(rows[1].contains("0.03125"));

        let output = directory.path().join("usage.csv");
        std::fs::write(&output, "keep me").unwrap();
        let (success, _, error) = run(&[
            "nummetria",
            "--database",
            database,
            "export",
            "--format",
            "csv",
            "--output",
            output.to_str().unwrap(),
        ]);
        assert!(!success);
        assert!(error.contains("could not create"));
        assert_eq!(std::fs::read_to_string(output).unwrap(), "keep me");
    }
}
