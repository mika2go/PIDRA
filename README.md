# PIDRA

<img src="assets/pidra.png" width="64" alt="PIDRA icon">

PIDRA is a small Linux TUI for answering two questions:

1. Which desktop app is using all that memory?
2. What actually happened after I tried to stop it?

It deliberately shows GUI applications in the main list instead of dumping
every process on the machine. Open **Details** when you need the full child
process tree, command, cgroup, resource usage or PIDRA's close-risk notes.

```text
PIDRA                         6 GUI PROCESSES              CPU 18  MEM 35

PROCESS NAME                    ID       SIZE      RESTART   STOP   DETAILS
>spotify                       2031     447 MB       [R]      [S]      [D]
 zen                         128870     998 MB       [R]      [S]      [D]

UP/DOWN ROW  LEFT/RIGHT ACTION  ENTER USE  / SEARCH  H HISTORY  ? HELP
```

## Install

You need a current Rust toolchain and Linux.

```bash
git clone https://github.com/mika2go/PIDRA.git
cd PIDRA
cargo install --path . --locked
```

Then start it with:

```bash
pidra
```

If Cargo's bin directory is not in your `PATH`, either add `~/.cargo/bin` or
link the binary into `~/.local/bin`.

## Controls

| Key | What it does |
| --- | --- |
| `↑` / `↓` | Select a process or child process |
| `←` / `→` | Select an action; expand/collapse in Details |
| `Enter` | Run the selected action |
| `/` | Search by name or PID |
| `R`, `S`, `D` | Focus Restart, Stop or Details |
| `F`, `T` | Freeze/resume or send SIGTERM in Details |
| `Shift+K` | Open the Force Stop confirmation |
| `H` | Show this session's action history |
| `?` | Open help |
| `Q` | Quit |

Mouse actions use a deliberate two-step click: the first click selects the
button, the second click runs it. `Enter` does the same after the first click.
This keeps an accidental click from stopping an application.

## A few important details

- **Stop is SIGTERM.** PIDRA never turns it into SIGKILL behind your back.
- **Force Stop is explicit.** It only exists in Details and always asks again.
- Every signal checks both the PID and its `/proc` start time. A reused PID is
  rejected.
- PID 1, PIDRA itself, its parent chain and essential desktop-session processes
  are blocked.
- A green-looking risk assessment is not a promise. An application can still
  lose unsaved work.
- Restart uses a real systemd user service when one exists. Transient app
  scopes cannot be started again by systemd, so PIDRA falls back to a guarded
  direct restart only when it has an absolute executable and working directory.

Spotify and other Chromium/Electron apps often have many helper processes and
may handle SIGTERM themselves. If one stays alive, PIDRA reports **STILL
RUNNING**; it does not silently kill the remaining process tree.

## Options

```text
--no-mouse
--no-color
--ascii
--refresh <milliseconds>
--pid <pid>
```

Configuration is optional. PIDRA reads
`$XDG_CONFIG_HOME/pidra/config.toml` or `~/.config/pidra/config.toml`.
The defaults are shown in [BUILDPLAN.md](BUILDPLAN.md).

Logs go to `$XDG_STATE_HOME/pidra/pidra.log` or
`~/.local/state/pidra/pidra.log`. They are useful when an app ignores a signal
or a restart source turns out to be unavailable.

## Building and testing

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
```

The implementation rules are in [BUILDPLAN.md](BUILDPLAN.md), and the manual
terminal checks are in [docs/SMOKE_TESTS.md](docs/SMOKE_TESTS.md).

PIDRA currently targets Linux only. No license has been chosen yet.
