use std::process::ExitCode;

use clap::Parser;
use pidra::{
    app::App,
    cli::{Cli, CliCommand},
    config::Config,
    control::{ControlWorker, RestartWorker},
    event,
    history::ActionHistory,
    logging,
    process::ScanWorker,
    terminal::{TerminalSession, install_panic_hook},
    tui::RenderOptions,
};

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pidra: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(CliCommand::Inspect(arguments)) = &cli.command {
        let report = pidra::inspect::inspect_system(arguments.pid)?;
        println!("{}", report.render(arguments.json)?);
        return Ok(());
    }
    let (config, config_path) = Config::load_default()?;
    let log_path = logging::initialize()?;
    let refresh_interval = cli.refresh_interval(config.refresh_interval_ms);
    let mouse_enabled = config.mouse && !cli.no_mouse;
    let no_color = config.no_color(cli.no_color);
    let ascii = cli.ascii || !config.unicode;
    tracing::info!(
        config = %config_path.display(),
        log = %log_path.display(),
        refresh_ms = refresh_interval.as_millis(),
        mouse_enabled,
        "PIDRA starting"
    );
    let mut session = TerminalSession::enter(mouse_enabled)?;
    install_panic_hook(mouse_enabled);

    let history = if config.persistent_history {
        ActionHistory::persistent_default(config.history_capacity).unwrap_or_else(|error| {
            tracing::warn!(error = %error, "persistent history unavailable; using session history");
            ActionHistory::new(config.history_capacity)
        })
    } else {
        ActionHistory::new(config.history_capacity)
    };
    let mut app = App::with_history(history);
    app.request_initial_pid(cli.pid);
    let scanner = ScanWorker::spawn_system(refresh_interval);
    let control = ControlWorker::spawn();
    let restart = RestartWorker::spawn();
    let options = RenderOptions { ascii, no_color };

    event::run(
        session.terminal_mut(),
        &mut app,
        &scanner,
        &control,
        &restart,
        options,
        refresh_interval,
    )?;
    session.restore()?;
    tracing::info!("PIDRA stopped cleanly");
    Ok(())
}
