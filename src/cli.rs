use std::time::Duration;

use clap::Parser;

const DEFAULT_REFRESH_MS: u64 = 1_000;
const MIN_REFRESH_MS: u64 = 100;
const MAX_REFRESH_MS: u64 = 60_000;

/// PIDRA is a keyboard-first Linux terminal process manager.
#[derive(Debug, Clone, Parser)]
#[command(author, version, about)]
pub struct Cli {
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
        default_value_t = DEFAULT_REFRESH_MS,
        value_parser = parse_refresh
    )]
    pub refresh_ms: u64,

    /// Open Details for a process after the process backend is enabled.
    #[arg(long, value_name = "PID")]
    pub pid: Option<i32>,
}

impl Cli {
    #[must_use]
    pub fn refresh_interval(&self) -> Duration {
        Duration::from_millis(self.refresh_ms)
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
        assert_eq!(cli.refresh_ms, 250);
    }

    #[test]
    fn rejects_an_excessive_refresh_rate() {
        let error = Cli::try_parse_from(["pidra", "--refresh", "20"])
            .expect_err("20 ms is below the supported minimum");

        assert!(error.to_string().contains("between 100 and 60000"));
    }
}
