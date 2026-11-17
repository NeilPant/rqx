use std::time::{Duration, Instant};

/// Frames per second while something is moving. The loop blocks on input when
/// nothing is, so this is a ceiling on a busy screen rather than a steady wakeup.
pub const FRAME: Duration = Duration::from_millis(33);
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// A clock, not an engine: how long the session has been running, which is what the
/// owl's glance and the spinner read to pick their frame.
#[derive(Copy, Clone)]
pub struct Motion {
    started: Instant,
    reduced: bool,
}

impl Motion {
    pub fn new(reduced: bool) -> Self {
        Self {
            started: Instant::now(),
            reduced,
        }
    }

    pub const fn reduced(&self) -> bool {
        self.reduced
    }

    /// Time since the clock started.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn spinner(&self) -> char {
        if self.reduced {
            return '·';
        }
        let step = (self.started.elapsed().as_millis() / 80) as usize;
        SPINNER[step % SPINNER.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spinner_stops_turning_when_motion_is_off() {
        assert_eq!(Motion::new(true).spinner(), '·');
        assert!(SPINNER.contains(&Motion::new(false).spinner()));
    }
}
