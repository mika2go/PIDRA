use std::time::Duration;

use clap::{Args, Parser, Subcommand};

pub const DEFAULT_REFRESH_MS: u64 = 1_000;
pub const MIN_REFRESH_MS: u64 = 100;
pub const MAX_REFRESH_MS: u64 = 60_000;

/// PIDRA is a keyboard-first Linux terminal process manager.
#[derive(Debug, Clone, Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<CliCommand>,

    /// Disable terminal mouse capture.
    #[arg(long)]
    pub no_mouse: bool,

    /// Disable colored output.
    #[arg(long)]
    pub no_color: bool,

    /// Use ASCII-only symbols.
    #[arg(long)]
    pub ascii: bool,

    /// Refresh interval in milliseconds.
    #[arg(
        long = "refresh",
        value_name = "MILLISECONDS",
        value_parser = parse_refresh
    )]
    pub refresh_ms: Option<u64>,

    /// Open Details for a process after the process backend is enabled.
    #[arg(long, value_name = "PID")]
    pub pid: Option<i32>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum CliCommand {
    /// Inspect one process without entering the TUI or changing process state.
    Inspect(InspectArgs),
}

#[derive(Debug, Clone, Args)]
pub struct InspectArgs {
    /// Process ID to inspect.
    #[arg(long, value_name = "PID", value_parser = parse_pid)]
    pub pid: i32,

    /// Emit versioned JSON instead of the human-readable report.
    #[arg(long)]
    pub json: bool,
}

impl Cli {
    #[must_use]
    pub fn refresh_interval(&self, configured_ms: u64) -> Duration {
        Duration::from_millis(self.refresh_ms.unwrap_or(configured_ms))
    }
}

fn parse_refresh(value: &str) -> Result<u64, String> {
    let milliseconds = value
        .parse::<u64>()
        .map_err(|_| "refresh interval must be an integer".to_owned())?;
    if !(MIN_REFRESH_MS..=MAX_REFRESH_MS).contains(&milliseconds) {
        return Err(format!(
            "refresh interval must be between {MIN_REFRESH_MS} and {MAX_REFRESH_MS} milliseconds"
        ));
    }
    Ok(milliseconds)
}

fn parse_pid(value: &str) -> Result<i32, String> {
    value
        .parse::<i32>()
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| "PID must be a positive integer".to_owned())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Cli;

    #[test]
    fn parses_phase_zero_flags() {
        let cli = Cli::try_parse_from([
            "pidra",
            "--no-mouse",
            "--no-color",
            "--ascii",
            "--refresh",
            "250",
        ])
        .expect("valid CLI");

        assert!(cli.no_mouse);
        assert!(cli.no_color);
        assert!(cli.ascii);
        assert_eq!(cli.refresh_ms, Some(250));
        assert!(cli.command.is_none());
    }

    #[test]
    fn parses_read_only_inspection_subcommand() {
        let cli = Cli::try_parse_from(["pidra", "inspect", "--pid", "42", "--json"])
            .expect("inspect command");
        let Some(super::CliCommand::Inspect(arguments)) = cli.command else {
            panic!("expected inspect command");
        };
        assert_eq!(arguments.pid, 42);
        assert!(arguments.json);
    }

    #[test]
    fn rejects_an_excessive_refresh_rate() {
        let error = Cli::try_parse_from(["pidra", "--refresh", "20"])
            .expect_err("20 ms is below the supported minimum");

        assert!(error.to_string().contains("between 100 and 60000"));
    }

    #[test]
    fn rejects_non_positive_inspection_pid() {
        let error = Cli::try_parse_from(["pidra", "inspect", "--pid", "0"])
            .expect_err("PID zero is invalid");
        assert!(error.to_string().contains("positive integer"));
    }
}
