//! Terminal setup and restoration, plus key decoding.
use std::io::{self, IsTerminal, Stdout};
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::style::ResetColor;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub type Term = Terminal<CrosstermBackend<Stdout>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Up,
    Down,
    Enter,
    Escape,
    Interrupt,
    Char(char),
    /// Any other key: cancels a prompt, otherwise ignored.
    Other,
}

const SIGNALS: [libc::c_int; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];
const END: &[u8] = b"\x1b[0m\x1b[?25h\x1b[?1049l";

static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Terminal attributes before raw mode, for the async-signal-safe handler.
static mut SAVED: MaybeUninit<libc::termios> = MaybeUninit::uninit();

extern "C" fn interrupted(sig: libc::c_int) {
    // Only async-signal-safe calls: restore the saved attributes, reset the screen, exit.
    unsafe {
        if ACTIVE.load(Ordering::SeqCst) {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, (&raw const SAVED).cast());
            libc::write(libc::STDOUT_FILENO, END.as_ptr().cast(), END.len());
        }
        libc::_exit(128 + sig);
    }
}

/// Raw mode and the alternate screen while alive; restored on drop, on panic and on
/// `SIGINT` / `SIGTERM` / `SIGHUP`. Resizes arrive as crossterm events.
pub struct Guard {
    old: [libc::sigaction; 3],
}

impl Guard {
    pub fn enter() -> io::Result<(Guard, Term)> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(io::Error::other("not a terminal"));
        }
        // SAFETY: single-threaded setup before any handler is installed.
        let mut old: [libc::sigaction; 3] = unsafe { std::mem::zeroed() };
        unsafe {
            if libc::tcgetattr(libc::STDIN_FILENO, (&raw mut SAVED).cast()) != 0 {
                return Err(io::Error::last_os_error());
            }
            let mut stop: libc::sigaction = std::mem::zeroed();
            stop.sa_sigaction = interrupted as *const () as libc::sighandler_t;
            libc::sigemptyset(&mut stop.sa_mask);
            for (sig, old) in SIGNALS.iter().zip(&mut old) {
                libc::sigaction(*sig, &stop, old);
            }
        }
        let guard = Guard { old };
        terminal::enable_raw_mode()?;
        ACTIVE.store(true, Ordering::SeqCst);
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            leave();
            hook(info);
        }));
        execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        let term = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok((guard, term))
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        leave();
        // SAFETY: restores the dispositions saved in `enter`.
        unsafe {
            for (sig, old) in SIGNALS.iter().zip(&self.old) {
                libc::sigaction(*sig, old, std::ptr::null_mut());
            }
        }
    }
}

fn leave() {
    if ACTIVE.swap(false, Ordering::SeqCst) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), ResetColor, Show, LeaveAlternateScreen);
    }
}

pub fn read_key(timeout: Duration) -> io::Result<Option<Key>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }
    let Event::Key(k) = event::read()? else { return Ok(None) };
    if k.kind == KeyEventKind::Release {
        return Ok(None);
    }
    let control = k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    Ok(Some(match k.code {
        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => Key::Interrupt,
        KeyCode::Char(c) if !control => Key::Char(c),
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Escape,
        _ => Key::Other,
    }))
}

/// Control bytes from log data must not reach the terminal.
pub fn safe(s: &str) -> String {
    s.chars().map(|c| if c < ' ' || c == '\x7f' { '?' } else { c }).collect()
}
