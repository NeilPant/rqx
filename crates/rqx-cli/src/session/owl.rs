//! What the owl is doing, and which frame of the sprite shows it.
//!
//! While a job runs it reads: the pupils drop and sweep along a line, jump back for the
//! next, and then the owl looks up and around. Otherwise it is the still frame the
//! header carries. With motion off it is the still frame always. The plumage follows
//! the verdict, which is the caller's to say: this only picks the frame.
//!
//! Every sequence is a table of `(frame, ticks)` rows, a tick being one `motion::FRAME`.

use std::time::Duration;

use ratatui::text::Line;

use crate::brand::{Ink, Palette};

use super::motion::{FRAME, Motion};
use super::sprite::Sprite;

/// One row of a sequence: a frame of the sprite and how many ticks it holds.
type Step = (&'static str, u32);

/// Two lines read left to right, then a look up and around.
const READ: [Step; 11] = [
    ("look-down-left", 9),
    ("look-down", 5),
    ("look-down-right", 11),
    ("look-down-left", 9),
    ("look-down", 5),
    ("look-down-right", 11),
    ("idle", 10),
    ("glance-left", 8),
    ("idle", 3),
    ("glance-right", 8),
    ("idle", 5),
];
/// `READ` with a blink inside the look up, played every third cycle.
const READ_BLINK: [Step; 13] = [
    ("look-down-left", 9),
    ("look-down", 5),
    ("look-down-right", 11),
    ("look-down-left", 9),
    ("look-down", 5),
    ("look-down-right", 11),
    ("idle", 4),
    ("blink", 3),
    ("idle", 3),
    ("glance-left", 8),
    ("idle", 3),
    ("glance-right", 8),
    ("idle", 5),
];
/// Cycles before the reading loop repeats exactly: two plain ones and one that blinks.
const READ_CYCLES: u32 = 3;

/// The frame `tick` ticks into `steps`, or `None` once they are over.
fn frame_in(steps: &[Step], tick: u32) -> Option<&'static str> {
    let mut end = 0;
    steps.iter().find_map(|&(frame, ticks)| {
        end += ticks;
        (tick < end).then_some(frame)
    })
}

fn length(steps: &[Step]) -> u32 {
    steps.iter().map(|&(_, ticks)| ticks).sum()
}

/// Whole ticks in `elapsed`.
fn ticks(elapsed: Duration) -> u32 {
    u32::try_from(elapsed.as_millis() / FRAME.as_millis()).unwrap_or(u32::MAX)
}

pub struct Owl {
    sprite: Sprite,
}

impl Owl {
    pub fn new(sprite: Sprite) -> Self {
        Self { sprite }
    }

    /// The reading frame `elapsed` after the job started. A pure function of time, so
    /// the loop needs no state and always starts at the beginning of a line.
    fn frame_at(elapsed: Duration) -> &'static str {
        let tick = ticks(elapsed);
        let cycle = length(&READ);
        let table: &[Step] = if (tick / cycle) % READ_CYCLES == READ_CYCLES - 1 {
            &READ_BLINK
        } else {
            &READ
        };
        frame_in(table, tick % cycle).unwrap_or("idle")
    }

    /// The owl as it looks while a job runs, reading. `motion` is the job's own clock,
    /// so the loop starts when the job does.
    pub fn glancing(&self, motion: &Motion, palette: Palette, signal: Ink) -> Vec<Line<'static>> {
        let name = if motion.reduced() {
            "idle"
        } else {
            Self::frame_at(motion.elapsed())
        };
        self.posed(name, palette, signal)
    }

    /// The still owl a header carries, which cannot move because scrollback cannot.
    pub fn still(&self, palette: Palette, signal: Ink) -> Vec<Line<'static>> {
        self.posed("idle", palette, signal)
    }

    fn posed(&self, name: &str, palette: Palette, signal: Ink) -> Vec<Line<'static>> {
        self.sprite
            .render_frame(palette, self.sprite.frame(name), signal, usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::ColorDepth;

    fn owl() -> Owl {
        Owl::new(Sprite::parse(include_str!(
            "../../assets/mascot-compact.txt"
        )))
    }

    fn reading(tick: u32) -> &'static str {
        Owl::frame_at(FRAME * tick)
    }

    #[test]
    fn a_reading_cycle_is_eighty_four_ticks_with_or_without_the_blink() {
        assert_eq!(length(&READ), 84);
        assert_eq!(length(&READ_BLINK), 84);
    }

    #[test]
    fn a_working_owl_reads_two_lines_then_looks_up_and_around() {
        assert_eq!(reading(0), "look-down-left", "the loop starts on a line");
        assert_eq!(reading(9), "look-down");
        assert_eq!(reading(14), "look-down-right");
        assert_eq!(
            reading(25),
            "look-down-left",
            "the jump back to the next line"
        );
        assert_eq!(reading(50), "idle", "the look up");
        assert_eq!(reading(60), "glance-left");
        assert_eq!(reading(71), "glance-right");
        assert_eq!(reading(83), "idle");
        assert_eq!(reading(84), "look-down-left", "the second cycle");
    }

    #[test]
    fn every_third_cycle_blinks_during_the_look_up() {
        let blinks = |cycle: u32| (0..84).any(|tick| reading(cycle * 84 + tick) == "blink");
        assert_eq!(
            [0, 1, 2, 3, 4, 5].map(blinks),
            [false, false, true, false, false, true]
        );
        assert_eq!(reading(2 * 84 + 54), "blink");
        assert_eq!(reading(2 * 84 + 57), "idle");
        assert_eq!(
            reading(3 * 84),
            "look-down-left",
            "the loop repeats after three cycles"
        );
    }

    #[test]
    fn with_motion_off_the_owl_is_the_still_frame() {
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let owl = owl();
        assert_eq!(
            owl.glancing(&Motion::new(true), palette, Ink::Good),
            owl.still(palette, Ink::Good)
        );
    }

    #[test]
    fn the_plumage_is_whatever_colour_the_caller_names() {
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let owl = owl();
        assert_ne!(
            owl.still(palette, Ink::Good),
            owl.still(palette, Ink::Danger)
        );
    }

    /// Every frame the session plays, in the order the asset holds them. `idle` is first
    /// because an unknown name falls back to frame 0.
    const FRAMES: [&str; 17] = [
        "idle",
        "blink",
        "glance-left",
        "glance-right",
        "sleep",
        "drowsy",
        "half",
        "look-down",
        "look-down-left",
        "look-down-right",
        "twitch-left",
        "twitch-right",
        "breathe",
        "happy",
        "content",
        "worried",
        "alarm",
    ];

    #[test]
    fn the_compact_owl_is_eight_by_six_pixels_in_every_frame() {
        let sprite = Sprite::parse(include_str!("../../assets/mascot-compact.txt"));
        assert_eq!(sprite.width(), 8);
        assert_eq!(sprite.cell_height(), 3);
        assert_eq!(sprite.frames(), FRAMES.len());
        for (at, name) in FRAMES.iter().enumerate() {
            assert_eq!(sprite.frame(name), at, "{name} is not where it should be");
            let pixels = sprite.pixels(at);
            assert_eq!(pixels.len(), 6, "{name} is not six pixels tall");
            assert!(
                pixels.iter().all(|row| row.len() == 8),
                "{name} is not eight pixels wide"
            );
        }
    }
}
