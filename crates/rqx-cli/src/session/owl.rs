//! What the owl is doing, and which frame of the sprite shows it.
//!
//! While a job runs it looks left and right, which says the session is busy without a
//! word. Otherwise it is the still frame the header carries. With motion off it is the
//! still frame always. The plumage follows the verdict, which is the caller's to say:
//! this only picks the frame.

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

pub struct Owl {
    sprite: Sprite,
}

impl Owl {
    pub fn new(sprite: Sprite) -> Self {
        Self { sprite }
    }

    /// The frame to show `elapsed` after the clock started.
    fn frame_at(elapsed: Duration) -> &'static str {
        let step = elapsed.as_millis() / GLANCE_STEP.as_millis();
        GLANCE[step as usize % GLANCE.len()]
    }

    /// The owl as it looks while a job runs, looking about.
    pub fn glancing(&self, motion: &Motion, palette: Palette, signal: Ink) -> Vec<Line<'static>> {
        let name = if motion.reduced() {
            "idle"
        } else {
            Self::frame_at(motion.elapsed())
        };
        self.sprite
            .render_frame(palette, self.sprite.frame(name), signal, usize::MAX)
    }

    /// The still owl a header carries, which cannot move because scrollback cannot.
    pub fn still(&self, palette: Palette, signal: Ink) -> Vec<Line<'static>> {
        self.sprite
            .render_frame(palette, self.sprite.frame("idle"), signal, usize::MAX)
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

    #[test]
    fn a_working_owl_looks_ahead_then_left_then_ahead_then_right() {
        let looks: Vec<&str> = (0..GLANCE.len() as u32)
            .map(|step| Owl::frame_at(GLANCE_STEP * step))
            .collect();
        assert_eq!(looks, GLANCE);
        assert!(looks.contains(&"glance-left") && looks.contains(&"glance-right"));
        assert_eq!(
            Owl::frame_at(GLANCE_STEP * GLANCE.len() as u32),
            "idle",
            "the look repeats"
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
