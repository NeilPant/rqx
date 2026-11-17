//! What the owl is doing, and which frame of the sprite shows it.
//!
//! While a job runs it looks left and right, which says the session is busy without a
//! word. At rest it blinks at random, three to seven seconds apart. With motion off it is
//! the still frame and nothing else. The plumage follows the verdict, which is the
//! caller's to say: this only picks the frame.

use std::time::Duration;

use ratatui::text::Line;

use crate::brand::{Ink, Palette};

use super::motion::Motion;
use super::sprite::Sprite;

/// How long a glance holds before the eyes move on.
const GLANCE_STEP: Duration = Duration::from_millis(450);
/// The look, in order: ahead, left, left, ahead, right, right.
const GLANCE: [&str; 6] = [
    "idle",
    "glance-left",
    "glance-left",
    "idle",
    "glance-right",
    "glance-right",
];
/// How long the eyes stay shut. Short enough to read as a blink and not as a nap.
const BLINK: Duration = Duration::from_millis(160);
/// Between one blink ending and the next beginning.
const SHORTEST_GAP_MS: u64 = 3_000;
const LONGEST_GAP_MS: u64 = 7_000;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Pose {
    Working,
    Resting,
}

pub struct Owl {
    sprite: Sprite,
    blinks: Blinks,
}

impl Owl {
    /// `seed` decides when the blinks fall. Anything will do; the clock is what the
    /// session gives it, and a test gives it a number so the blinks are the same each run.
    pub fn new(sprite: Sprite, seed: u64) -> Self {
        Self {
            sprite,
            blinks: Blinks::new(seed),
        }
    }

    /// The frame to show `elapsed` after the clock started.
    fn frame_at(&mut self, pose: Pose, elapsed: Duration) -> &'static str {
        match pose {
            Pose::Working => {
                let step = elapsed.as_millis() / GLANCE_STEP.as_millis();
                GLANCE[step as usize % GLANCE.len()]
            }
            Pose::Resting if self.blinks.shut_at(elapsed) => "blink",
            Pose::Resting => "idle",
        }
    }

    pub fn lines(
        &mut self,
        pose: Pose,
        motion: &Motion,
        palette: Palette,
        signal: Ink,
    ) -> Vec<Line<'static>> {
        let name = if motion.reduced() {
            "idle"
        } else {
            self.frame_at(pose, motion.elapsed())
        };
        self.sprite
            .render_frame(palette, self.sprite.frame(name), signal, usize::MAX)
    }

    /// The still owl a header carries, which never blinks because scrollback cannot.
    pub fn still(&self, palette: Palette, signal: Ink) -> Vec<Line<'static>> {
        self.sprite
            .render_frame(palette, self.sprite.frame("idle"), signal, usize::MAX)
    }

    /// How long until the owl next looks different, so the loop can sleep exactly that
    /// long instead of waking on a timer to find nothing changed.
    pub fn changes_in(&self, pose: Pose, motion: &Motion) -> Option<Duration> {
        if motion.reduced() {
            return None;
        }
        Some(match pose {
            Pose::Working => GLANCE_STEP,
            Pose::Resting => self.blinks.changes_in(motion.elapsed()),
        })
    }
}

/// Spreads a seed over all its bits, so seeds that are close together do not blink
/// close together. This is splitmix64's finaliser.
fn scramble(seed: u64) -> u64 {
    let mut mixed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^ (mixed >> 31)
}

/// When the eyes are shut. A small generator of its own rather than a dependency: the
/// only thing it has to be is uneven.
struct Blinks {
    state: u64,
    /// When the next blink starts, measured from the clock's start.
    next: Duration,
}

impl Blinks {
    fn new(seed: u64) -> Self {
        let mut blinks = Self {
            // A zero state would stay zero for ever.
            state: scramble(seed) | 1,
            next: Duration::ZERO,
        };
        blinks.next = blinks.gap();
        blinks
    }

    fn gap(&mut self) -> Duration {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        let span = LONGEST_GAP_MS - SHORTEST_GAP_MS + 1;
        let mixed = self.state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32;
        Duration::from_millis(SHORTEST_GAP_MS + mixed % span)
    }

    /// Whether the eyes are shut at `elapsed`. A call after a long sleep skips the
    /// blinks that fell in it rather than showing them late.
    fn shut_at(&mut self, elapsed: Duration) -> bool {
        while elapsed >= self.next + BLINK {
            let gap = self.gap();
            self.next += BLINK + gap;
        }
        elapsed >= self.next
    }

    fn changes_in(&self, elapsed: Duration) -> Duration {
        if elapsed >= self.next {
            (self.next + BLINK).saturating_sub(elapsed)
        } else {
            self.next - elapsed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::ColorDepth;

    fn owl(seed: u64) -> Owl {
        Owl::new(Sprite::parse(include_str!("../../assets/mascot.txt")), seed)
    }

    fn seconds(value: f64) -> Duration {
        Duration::from_secs_f64(value)
    }

    #[test]
    fn a_working_owl_looks_ahead_then_left_then_ahead_then_right() {
        let mut owl = owl(1);
        let looks: Vec<&str> = (0..GLANCE.len() as u64)
            .map(|step| owl.frame_at(Pose::Working, GLANCE_STEP * step as u32))
            .collect();
        assert_eq!(looks, GLANCE);
        assert!(looks.contains(&"glance-left") && looks.contains(&"glance-right"));
        assert_eq!(
            owl.frame_at(Pose::Working, GLANCE_STEP * GLANCE.len() as u32),
            "idle",
            "the look repeats"
        );
    }

    #[test]
    fn a_resting_owl_has_its_eyes_open_until_its_first_blink() {
        let mut owl = owl(7);
        let first = owl.blinks.next;
        assert!(first >= seconds(3.0) && first <= seconds(7.0), "{first:?}");
        assert_eq!(owl.frame_at(Pose::Resting, Duration::ZERO), "idle");
        assert_eq!(owl.frame_at(Pose::Resting, first - seconds(0.01)), "idle");
        assert_eq!(owl.frame_at(Pose::Resting, first), "blink");
        assert_eq!(owl.frame_at(Pose::Resting, first + BLINK), "idle");
    }

    #[test]
    fn blinks_fall_three_to_seven_seconds_apart() {
        let mut blinks = Blinks::new(0x5eed);
        let mut previous_end = Duration::ZERO;
        for _ in 0..200 {
            let start = blinks.next;
            let gap = start - previous_end;
            assert!(
                gap >= seconds(3.0) && gap <= seconds(7.0),
                "a gap of {gap:?}"
            );
            previous_end = start + BLINK;
            blinks.shut_at(previous_end);
        }
    }

    #[test]
    fn different_seeds_blink_at_different_moments() {
        let firsts: std::collections::BTreeSet<Duration> =
            (1..20).map(|seed| Blinks::new(seed).next).collect();
        assert!(firsts.len() > 10, "{firsts:?}");
    }

    #[test]
    fn a_long_sleep_skips_the_blinks_it_missed() {
        let mut blinks = Blinks::new(3);
        let later = seconds(600.0);
        blinks.shut_at(later);
        assert!(blinks.next + BLINK > later);
    }

    #[test]
    fn the_loop_is_told_how_long_it_may_sleep() {
        let blinks = Blinks::new(9);
        let first = blinks.next;
        assert_eq!(blinks.changes_in(Duration::ZERO), first);
        assert_eq!(blinks.changes_in(first), BLINK);
        assert_eq!(
            blinks.changes_in(first + seconds(0.1)),
            BLINK - seconds(0.1)
        );
    }

    #[test]
    fn with_motion_off_the_owl_is_the_still_frame_and_never_wakes_the_loop() {
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let still = Motion::new(true);
        let mut owl = owl(5);
        for pose in [Pose::Working, Pose::Resting] {
            assert_eq!(owl.changes_in(pose, &still), None);
            assert_eq!(
                owl.lines(pose, &still, palette, Ink::Good),
                owl.still(palette, Ink::Good)
            );
        }
    }

    #[test]
    fn the_plumage_is_whatever_colour_the_caller_names() {
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let owl = owl(5);
        assert_ne!(
            owl.still(palette, Ink::Good),
            owl.still(palette, Ink::Danger)
        );
    }
}
