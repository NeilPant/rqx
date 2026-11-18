//! The busy owl's reaction to a scan that lands after the header has gone into
//! scrollback. The busy rows stay for the length of the verdict's reaction, with the owl
//! beside a one-line result where the spinner was, and then give way to the prompt.
//! Nothing here draws: it knows when a reaction starts, what it shows and when it is over.

use std::time::{Duration, Instant};

use crate::brand::{self, Ink};

use super::MIN_HEIGHT;
use super::owl::Verdict;

pub struct Reaction {
    started: Instant,
    verdict: Verdict,
    health: u32,
    findings: u32,
}

impl Reaction {
    /// The reaction to a scan that just landed, or nothing when motion is off or the
    /// window is too short to hold the rows it would keep.
    pub fn after_scan(health: u32, findings: u32, reduced: bool, rows: u16) -> Option<Self> {
        if reduced || rows < MIN_HEIGHT {
            return None;
        }
        Some(Self {
            started: Instant::now(),
            verdict: Verdict::of(Some(health)),
            health,
            findings,
        })
    }

    pub const fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// The plumage and the result line wear the verdict's colour.
    pub const fn ink(&self) -> Ink {
        brand::mascot_ink(Some(self.health))
    }

    pub const fn health(&self) -> u32 {
        self.health
    }

    pub const fn findings(&self) -> u32 {
        self.findings
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn is_over(&self) -> bool {
        self.is_over_at(self.elapsed())
    }

    fn is_over_at(&self, elapsed: Duration) -> bool {
        elapsed >= self.verdict.reaction_time()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::motion::FRAME;

    const TALL: u16 = 30;

    fn after_scan(health: u32) -> Reaction {
        Reaction::after_scan(health, 12, false, TALL).expect("a reaction")
    }

    #[test]
    fn a_scan_starts_the_reaction_of_its_verdict() {
        assert_eq!(after_scan(90).verdict(), Verdict::Good);
        assert_eq!(after_scan(65).verdict(), Verdict::Fair);
        assert_eq!(after_scan(46).verdict(), Verdict::Poor);
    }

    #[test]
    fn the_plumage_follows_the_new_verdict() {
        assert_eq!(after_scan(90).ink(), Ink::Accent);
        assert_eq!(after_scan(65).ink(), Ink::Warn);
        assert_eq!(after_scan(46).ink(), Ink::Danger);
    }

    #[test]
    fn it_carries_what_the_result_line_quotes() {
        let reaction = after_scan(46);
        assert_eq!((reaction.health(), reaction.findings()), (46, 12));
    }

    #[test]
    fn with_motion_off_there_is_no_reaction() {
        assert!(Reaction::after_scan(46, 12, true, TALL).is_none());
    }

    #[test]
    fn a_window_too_short_for_the_session_has_no_reaction() {
        assert!(Reaction::after_scan(46, 12, false, MIN_HEIGHT - 1).is_none());
        assert!(Reaction::after_scan(46, 12, false, MIN_HEIGHT).is_some());
    }

    #[test]
    fn it_lasts_exactly_the_verdicts_reaction() {
        for health in [90, 65, 46] {
            let reaction = after_scan(health);
            let length = reaction.verdict().reaction_time();
            assert_eq!(length, FRAME * 24);
            assert!(!reaction.is_over_at(Duration::ZERO));
            assert!(!reaction.is_over_at(length - Duration::from_millis(1)));
            assert!(reaction.is_over_at(length));
        }
    }
}
