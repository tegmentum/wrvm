//! Native terminal progress: determinate download bar + indeterminate spinner.

use crate::util::human_bytes;
use std::io::{IsTerminal, Write};

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const BAR_WIDTH: usize = 24;

pub fn stderr_is_terminal() -> bool {
    std::io::stderr().is_terminal()
}

pub fn stdout_is_terminal() -> bool {
    std::io::stdout().is_terminal()
}

fn redraw(line: &str) {
    let mut err = std::io::stderr();
    let _ = write!(err, "\r\x1b[2K{line}");
    let _ = err.flush();
}

fn clear_line() {
    let mut err = std::io::stderr();
    let _ = write!(err, "\r\x1b[2K");
    let _ = err.flush();
}

pub struct Bar {
    label: String,
    total: u64,
    tty: bool,
    frame: usize,
    last_pct: i32,
}

impl Bar {
    pub fn new(label: impl Into<String>, total: u64) -> Bar {
        let label = label.into();
        let tty = stderr_is_terminal();
        if !tty {
            let suffix = if total > 0 {
                format!(" ({})", human_bytes(total))
            } else {
                String::new()
            };
            eprintln!("{label}{suffix} …");
        }
        Bar {
            label,
            total,
            tty,
            frame: 0,
            last_pct: -1,
        }
    }

    pub fn set(&mut self, current: u64) {
        if !self.tty {
            return;
        }
        let pct = (current * 100)
            .checked_div(self.total)
            .map(|p| p as i32)
            .unwrap_or(-1);
        if pct == self.last_pct {
            return;
        }
        self.last_pct = pct;
        self.frame = (self.frame + 1) % FRAMES.len();
        let spin = FRAMES[self.frame];

        if let Some(filled) = (current as usize * BAR_WIDTH)
            .checked_div(self.total as usize)
            .map(|f| f.min(BAR_WIDTH))
        {
            let bar: String = "█".repeat(filled) + &"░".repeat(BAR_WIDTH - filled);
            redraw(&format!(
                "{spin} {} [{bar}] {pct:>3}%  {} / {}",
                self.label,
                human_bytes(current),
                human_bytes(self.total),
            ));
        } else {
            redraw(&format!("{spin} {} {}", self.label, human_bytes(current)));
        }
    }

    pub fn finish(self, summary: &str) {
        if self.tty {
            clear_line();
        }
        eprintln!("✓ {summary}");
    }
}

pub struct Spinner {
    label: String,
    tty: bool,
    frame: usize,
}

impl Spinner {
    pub fn new(label: impl Into<String>) -> Spinner {
        let label = label.into();
        let tty = stderr_is_terminal();
        if tty {
            redraw(&format!("{} {label} …", FRAMES[0]));
        } else {
            eprintln!("{label} …");
        }
        Spinner {
            label,
            tty,
            frame: 0,
        }
    }

    pub fn tick(&mut self, detail: &str) {
        if !self.tty {
            return;
        }
        self.frame = (self.frame + 1) % FRAMES.len();
        redraw(&format!("{} {} {detail}", FRAMES[self.frame], self.label));
    }

    pub fn finish(self, summary: &str) {
        if self.tty {
            clear_line();
        }
        eprintln!("✓ {summary}");
    }
}
