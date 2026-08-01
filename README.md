# PIDRA

![PIDRA icon](assets/pidra.png)

PIDRA is a keyboard-first Linux terminal process manager built with Rust,
Ratatui and Crossterm.

> [!IMPORTANT]
> The current repository contains the completed Phase 0 terminal scaffold. It
> renders fixture data and does not inspect, restart or signal real processes
> yet.

The authoritative implementation contract, phased acceptance criteria and
safety requirements live in [BUILDPLAN.md](BUILDPLAN.md).

## Run the Phase 0 fixture

```bash
cargo run -- --no-mouse
```

Use the arrow keys to select a row and action, `Enter` to activate the fixture
action and `Q` to exit.

## Validation

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Linux is the only initial target. No software license has been selected yet.
