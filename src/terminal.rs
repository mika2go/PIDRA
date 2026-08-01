use std::{io, panic};

use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

pub trait TerminalOps {
    fn enable_raw_mode(&mut self) -> io::Result<()>;
    fn enter_alternate_screen(&mut self) -> io::Result<()>;
    fn enable_mouse_capture(&mut self) -> io::Result<()>;
    fn hide_cursor(&mut self) -> io::Result<()>;
    fn disable_mouse_capture(&mut self) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
    fn disable_raw_mode(&mut self) -> io::Result<()>;
}

#[derive(Debug, Default)]
pub struct CrosstermOps;

impl TerminalOps for CrosstermOps {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        enable_raw_mode()
    }

    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnterAlternateScreen)
    }

    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnableMouseCapture)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), Hide)
    }

    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(io::stdout(), DisableMouseCapture)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), Show)
    }

    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), LeaveAlternateScreen)
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        disable_raw_mode()
    }
}

#[derive(Debug)]
pub struct TerminalGuard<O: TerminalOps> {
    ops: O,
    raw_mode: bool,
    alternate_screen: bool,
    mouse_capture: bool,
    cursor_hidden: bool,
    restored: bool,
}

impl<O: TerminalOps> TerminalGuard<O> {
    pub fn enter(ops: O, mouse_enabled: bool) -> io::Result<Self> {
        let mut guard = Self {
            ops,
            raw_mode: false,
            alternate_screen: false,
            mouse_capture: false,
            cursor_hidden: false,
            restored: false,
        };

        let enter_result = (|| {
            guard.ops.enable_raw_mode()?;
            guard.raw_mode = true;
            guard.ops.enter_alternate_screen()?;
            guard.alternate_screen = true;
            if mouse_enabled {
                guard.ops.enable_mouse_capture()?;
                guard.mouse_capture = true;
            }
            guard.ops.hide_cursor()?;
            guard.cursor_hidden = true;
            Ok(())
        })();

        if let Err(error) = enter_result {
            let _ = guard.restore();
            return Err(error);
        }

        Ok(guard)
    }

    pub fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }

        let mut first_error = None;
        if self.mouse_capture {
            record_error(&mut first_error, self.ops.disable_mouse_capture());
            self.mouse_capture = false;
        }
        if self.cursor_hidden {
            record_error(&mut first_error, self.ops.show_cursor());
            self.cursor_hidden = false;
        }
        if self.alternate_screen {
            record_error(&mut first_error, self.ops.leave_alternate_screen());
            self.alternate_screen = false;
        }
        if self.raw_mode {
            record_error(&mut first_error, self.ops.disable_raw_mode());
            self.raw_mode = false;
        }
        self.restored = true;

        first_error.map_or(Ok(()), Err)
    }
}

impl<O: TerminalOps> Drop for TerminalGuard<O> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn record_error(first_error: &mut Option<io::Error>, result: io::Result<()>) {
    if let Err(error) = result
        && first_error.is_none()
    {
        *first_error = Some(error);
    }
}

pub struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    guard: TerminalGuard<CrosstermOps>,
}

impl TerminalSession {
    pub fn enter(mouse_enabled: bool) -> io::Result<Self> {
        let guard = TerminalGuard::enter(CrosstermOps, mouse_enabled)?;
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok(Self { terminal, guard })
    }

    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }

    pub fn restore(&mut self) -> io::Result<()> {
        self.guard.restore()
    }
}

pub fn install_panic_hook(mouse_enabled: bool) {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        best_effort_restore(mouse_enabled);
        original_hook(panic_info);
    }));
}

fn best_effort_restore(mouse_enabled: bool) {
    if mouse_enabled {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
    let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
    let _ = disable_raw_mode();
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        io,
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
    };

    use super::{TerminalGuard, TerminalOps};

    #[derive(Clone)]
    struct RecordingOps {
        calls: Rc<RefCell<Vec<&'static str>>>,
    }

    impl RecordingOps {
        fn call(&self, name: &'static str) {
            self.calls.borrow_mut().push(name);
        }
    }

    impl TerminalOps for RecordingOps {
        fn enable_raw_mode(&mut self) -> io::Result<()> {
            self.call("enable_raw");
            Ok(())
        }

        fn enter_alternate_screen(&mut self) -> io::Result<()> {
            self.call("enter_alternate");
            Ok(())
        }

        fn enable_mouse_capture(&mut self) -> io::Result<()> {
            self.call("enable_mouse");
            Ok(())
        }

        fn hide_cursor(&mut self) -> io::Result<()> {
            self.call("hide_cursor");
            Ok(())
        }

        fn disable_mouse_capture(&mut self) -> io::Result<()> {
            self.call("disable_mouse");
            Ok(())
        }

        fn show_cursor(&mut self) -> io::Result<()> {
            self.call("show_cursor");
            Ok(())
        }

        fn leave_alternate_screen(&mut self) -> io::Result<()> {
            self.call("leave_alternate");
            Ok(())
        }

        fn disable_raw_mode(&mut self) -> io::Result<()> {
            self.call("disable_raw");
            Ok(())
        }
    }

    #[test]
    fn forced_panic_restores_terminal_in_reverse_order() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let ops = RecordingOps {
            calls: Rc::clone(&calls),
        };

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _guard = TerminalGuard::enter(ops, true).expect("enter terminal");
            panic!("forced panic");
        }));

        assert!(result.is_err());
        assert_eq!(
            calls.borrow().as_slice(),
            [
                "enable_raw",
                "enter_alternate",
                "enable_mouse",
                "hide_cursor",
                "disable_mouse",
                "show_cursor",
                "leave_alternate",
                "disable_raw",
            ]
        );
    }

    #[test]
    fn restoration_is_idempotent() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let ops = RecordingOps {
            calls: Rc::clone(&calls),
        };
        let mut guard = TerminalGuard::enter(ops, false).expect("enter terminal");

        guard.restore().expect("first restore");
        let call_count = calls.borrow().len();
        guard.restore().expect("second restore");

        assert_eq!(calls.borrow().len(), call_count);
    }
}
