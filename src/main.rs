use std::process::ExitCode;

use clap::Parser;
use pidra::{
    app::App,
    cli::Cli,
    control::{ControlWorker, RestartWorker},
    event,
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
    let mouse_enabled = !cli.no_mouse;
    let mut session = TerminalSession::enter(mouse_enabled)?;
    install_panic_hook(mouse_enabled);

    let mut app = App::new();
    let scanner = ScanWorker::spawn_system(cli.refresh_interval());
    let control = ControlWorker::spawn();
    let restart = RestartWorker::spawn();
    let options = RenderOptions {
        ascii: cli.ascii,
        no_color: cli.no_color,
    };

    event::run(
        session.terminal_mut(),
        &mut app,
        &scanner,
        &control,
        &restart,
        options,
        cli.refresh_interval(),
    )?;
    session.restore()?;
    Ok(())
}
