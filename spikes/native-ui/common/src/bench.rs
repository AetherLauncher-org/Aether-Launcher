//! `--bench animate <seconds>` / `--bench idle <seconds>`: run the window for a while, print one
//! line of numbers, quit. Nobody has to click anything, so the numbers can be repeated.
//!
//! `animate` spins the cube; `idle` shows it still, which is the test that a toolkit does not
//! redraw when nothing changes.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    /// A person is using it: animation on, a checkbox turns it off.
    Interactive,
    Animate(Duration),
    Idle(Duration),
}

impl Mode {
    #[must_use]
    pub fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let Some(at) = args.iter().position(|a| a == "--bench") else {
            return Self::Interactive;
        };
        let seconds = args
            .get(at + 2)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(5.0);
        let duration = Duration::from_secs_f64(seconds);
        match args.get(at + 1).map(String::as_str) {
            Some("idle") => Self::Idle(duration),
            _ => Self::Animate(duration),
        }
    }

    /// Whether the cube spins at the start.
    #[must_use]
    pub fn animated(self) -> bool {
        !matches!(self, Self::Idle(_))
    }

    #[must_use]
    pub fn duration(self) -> Option<Duration> {
        match self {
            Self::Interactive => None,
            Self::Animate(d) | Self::Idle(d) => Some(d),
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Animate(_) => "animate",
            Self::Idle(_) => "idle",
        }
    }
}
