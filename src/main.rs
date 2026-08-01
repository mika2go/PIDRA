use std::process::ExitCode;

use clap::Parser;
use pidra::{
    app::App,
    cli::Cli,
    event,
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

    let mut app = App::fixture();
    let options = RenderOptions {
        ascii: cli.ascii,
        no_color: cli.no_color,
    };

    event::run(
        session.terminal_mut(),
        &mut app,
        options,
        cli.refresh_interval(),
    )?;
    session.restore()?;
    Ok(())
}
