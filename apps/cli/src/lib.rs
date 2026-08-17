use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, CommandFactory, Parser, Subcommand};

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
    Import,
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

/// Parse process arguments and run the selected command.
pub fn run() -> ExitCode {
    run_with(Cli::parse())
}

fn run_with(cli: Cli) -> ExitCode {
    match cli.command {
        Command::Version => {
            println!(
                "{} {}",
                nummetria_core::PRODUCT_NAME,
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::SUCCESS
        }
        command => {
            eprintln!(
                "The '{}' command is part of the v0.1 contract but is not implemented in this build.",
                command_name(&command)
            );
            ExitCode::from(2)
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
        Command::Import => "import",
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

    use super::{Cli, Command};

    #[test]
    fn command_definition_is_valid() {
        super::command().debug_assert();
    }

    #[test]
    fn parses_every_public_command() {
        let commands = [
            "setup",
            "status",
            "collect",
            "usage",
            "providers",
            "budget",
            "import",
            "export",
            "config",
            "data",
            "doctor",
            "completion",
            "version",
        ];

        for name in commands {
            let cli = Cli::try_parse_from(["nummetria", name])
                .unwrap_or_else(|error| panic!("failed to parse {name}: {error}"));
            assert!(matches!(
                cli.command,
                Command::Setup
                    | Command::Status
                    | Command::Collect
                    | Command::Usage
                    | Command::Providers
                    | Command::Budget
                    | Command::Import
                    | Command::Export
                    | Command::Config
                    | Command::Data
                    | Command::Doctor
                    | Command::Completion
                    | Command::Version
            ));
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
}
