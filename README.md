# PIDRA

![PIDRA icon](assets/pidra.png)

PIDRA is a keyboard-first Linux terminal process manager built with Rust,
Ratatui and Crossterm. Its main table shows detected graphical application
roots; Details exposes expandable child processes, resource data, termination
risk evidence and action results.

The authoritative implementation contract and safety requirements live in
[BUILDPLAN.md](BUILDPLAN.md). Linux is the only supported platform for this
initial release. No software license has been selected yet.

## Current features

- GUI-root detection from Hyprland, X11/EWMH and systemd application scopes;
- asynchronous `/proc` scanning with stable PID/start-time identities;
- expandable subprocess trees and masked command arguments;
- evidence-backed close analysis that never promises data safety;
- identity-safe SIGTERM, SIGSTOP, SIGCONT and confirmed SIGKILL via pidfd where
  supported;
- guarded systemd user-service and direct-exec restart without a shell;
- restarted-process, surviving-child, zombie, D-state and permission diagnosis;
- in-session action history, ASCII/no-color modes and optional mouse input.

## Run

```bash
cargo run --release -- --no-mouse
```

Use `↑`/`↓` for rows, `←`/`→` for actions, `Enter` to activate, `/` to search,
`H` for history, `?` for help and `Q` to quit. `--pid <PID>` opens Details for a
specific identity after the first scan.

## Safety model

Stop always means SIGTERM and never escalates automatically. Force Stop is
available only in Details and always requires confirmation. Every process
signal validates PID plus process start time. PID 1, PIDRA itself, its ancestor
chain and essential graphical-session processes are protected. Even permitted
actions can lose unsaved user data; prefer closing an application normally.

## Configuration and logs

Optional configuration is read from
`$XDG_CONFIG_HOME/pidra/config.toml` or `~/.config/pidra/config.toml`:

```toml
refresh_interval_ms = 1000
mouse = true
color = "auto"
unicode = true
confirm_force_stop = true
show_kernel_threads = false
mask_command_secrets = true
```

CLI flags override configuration. Safety-critical confirmation and secret
masking cannot be disabled in this release. Logs are appended to
`$XDG_STATE_HOME/pidra/pidra.log` or `~/.local/state/pidra/pidra.log`, never to
the active TUI.

## Validation

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
```

Terminal smoke-test guidance is documented in
[docs/SMOKE_TESTS.md](docs/SMOKE_TESTS.md).
