use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::brand::{Ink, Palette};

/// Pixel art is authored as text, one character per pixel, and embedded at compile
/// time: the art needs many passes to look right, and nobody iterates on a wordmark
/// stored as a nested array of colour constants.
pub struct Sprite {
    frames: Vec<Frame>,
    width: usize,
    height: usize,
}

struct Frame {
    name: String,
    rows: Vec<Vec<Option<Ink>>>,
}

impl Sprite {
    /// `#: name` opens a frame; every other `#` line is a comment. Checked before the
    /// comment filter, which is why a frame marker can look like one.
    pub fn parse(source: &str) -> Self {
        let mut frames: Vec<Frame> = Vec::new();
        for line in source.lines().map(str::trim_end) {
            if let Some(name) = line.strip_prefix("#:") {
                frames.push(Frame { name: name.trim().to_owned(), rows: Vec::new() });
                continue;
            }
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if frames.is_empty() {
                frames.push(Frame { name: "idle".to_owned(), rows: Vec::new() });
            }
            let row = line.chars().map(Ink::from_sprite_char).collect();
            frames.last_mut().expect("a frame is open").rows.push(row);
        }
        if frames.is_empty() {
            frames.push(Frame { name: "idle".to_owned(), rows: Vec::new() });
        }

        // Held across every frame so a layout constraint never jitters mid-animation.
        let width = frames
            .iter()
            .flat_map(|frame| frame.rows.iter().map(Vec::len))
            .max()
            .unwrap_or(0);
        let height = frames.iter().map(|frame| frame.rows.len()).max().unwrap_or(0);
        Self { frames, width, height }
    }

    pub fn frame(&self, name: &str) -> usize {
        self.frames.iter().position(|frame| frame.name == name).unwrap_or(0)
    }

    #[cfg(test)]
    pub fn frames(&self) -> usize {
        self.frames.len()
    }

    pub const fn width(&self) -> u16 {
        self.width as u16
    }

    /// Two pixels share one cell vertically, so an odd-height sprite still needs a
    /// whole row for its last line.
    pub fn cell_height(&self) -> u16 {
        self.height.div_ceil(2) as u16
    }

    fn pixel(&self, frame: usize, x: usize, y: usize) -> Option<Ink> {
        self.frames
            .get(frame)?
            .rows
            .get(y)
            .and_then(|row| row.get(x).copied())
            .flatten()
    }

    /// Renders with half-blocks so each cell carries two vertically stacked pixels
    /// with independent colours; braille would double the resolution again but is
    /// monochrome per cell, which coloured art cannot use.
    /// `signal` replaces the `S` pixels, and `reveal` limits how many columns are
    /// drawn so the mascot can wipe in a column at a time.
    pub fn render_frame(
        &self,
        palette: Palette,
        frame: usize,
        signal: Ink,
        reveal: usize,
    ) -> Vec<Line<'static>> {
        let frame = frame % self.frames.len().max(1);
        (0..self.height)
            .step_by(2)
            .map(|y| {
                let spans = (0..self.width)
                    .map(|x| {
                        if x < reveal {
                            self.cell(frame, x, y, palette, signal)
                        } else {
                            Span::raw(" ")
                        }
                    })
                    .collect::<Vec<_>>();
                Line::from(spans)
            })
            .collect()
    }

    fn cell(&self, frame: usize, x: usize, y: usize, palette: Palette, signal: Ink) -> Span<'static> {
        let resolve = |ink: Ink| if ink == Ink::Signal { signal } else { ink };
        let top = self.pixel(frame, x, y).map(resolve);
        let bottom = self.pixel(frame, x, y + 1).map(resolve);

        // Without colour, two stacked pixels of different inks are still two pixels:
        // splitting them into a half-block would erase the lower one from the shape.
        if palette.is_monochrome() {
            let glyph = match (top.is_some(), bottom.is_some()) {
                (true, true) => "\u{2588}",
                (true, false) => "\u{2580}",
                (false, true) => "\u{2584}",
                (false, false) => " ",
            };
            return Span::styled(glyph, Style::new().fg(Color::Reset));
        }

        let (glyph, style) = match (top, bottom) {
            (None, None) => (" ", Style::new()),
            (Some(ink), None) => ("\u{2580}", Style::new().fg(palette.color(ink))),
            (None, Some(ink)) => ("\u{2584}", Style::new().fg(palette.color(ink))),
            (Some(upper), Some(lower)) if upper == lower => {
                ("\u{2588}", Style::new().fg(palette.color(upper)))
            }
            (Some(upper), Some(lower)) => (
                "\u{2580}",
                Style::new().fg(palette.color(upper)).bg(palette.color(lower)),
            ),
        };
        Span::styled(glyph, style)
    }
}

#[cfg(test)]
mod tests {
    use crate::brand::ColorDepth;
    use super::*;

    fn plain(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn pairs_of_rows_collapse_into_one_cell_row() {
        let sprite = Sprite::parse("TT\nTT\nTT\n");
        assert_eq!(sprite.cell_height(), 2, "3 pixel rows need 2 cell rows");
    }

    #[test]
    fn glyph_encodes_which_of_the_two_pixels_are_set() {
        let sprite = Sprite::parse("T.T.\n.TT.");
        let rendered = plain(&sprite.render_frame(Palette::with_depth(ColorDepth::Truecolor), 0, Ink::Signal, usize::MAX));
        assert_eq!(rendered, "\u{2580}\u{2584}\u{2588} ");
    }

    /// Two different inks stacked in one cell are drawn in colour as a half-block
    /// with a background; with no colour available that would delete the lower
    /// pixel, so monochrome must fall back to a full block.
    #[test]
    fn no_pixel_is_lost_when_colour_is_unavailable() {
        let sprite = Sprite::parse("DW\nKT");
        assert_eq!(plain(&sprite.render_frame(Palette::with_depth(ColorDepth::Monochrome), 0, Ink::Signal, usize::MAX)), "\u{2588}\u{2588}");
        assert_eq!(plain(&sprite.render_frame(Palette::with_depth(ColorDepth::Truecolor), 0, Ink::Signal, usize::MAX)), "\u{2580}\u{2580}");
    }

    #[test]
    fn monochrome_still_distinguishes_the_four_pixel_states() {
        let sprite = Sprite::parse("T.T.\n.TT.");
        let mono = plain(&sprite.render_frame(Palette::with_depth(ColorDepth::Monochrome), 0, Ink::Signal, usize::MAX));
        assert_eq!(mono, "\u{2580}\u{2584}\u{2588} ");
    }

    #[test]
    fn comments_and_blank_lines_are_not_pixels() {
        let sprite = Sprite::parse("# a comment\n\nTT\n\nTT\n");
        assert_eq!(sprite.cell_height(), 1);
    }

    #[test]
    fn a_frame_marker_opens_a_frame_and_a_plain_comment_does_not() {
        let sprite = Sprite::parse("# note\n#: one\nTT\n#: two\nDD\n");
        assert_eq!(sprite.frames(), 2);
        assert_eq!(sprite.frame("two"), 1);
        assert_eq!(sprite.frame("missing"), 0, "an unknown name falls back to the first frame");
    }

    /// Frames of different heights would move every widget below the mascot on the
    /// frame it changed, so the layout is sized to the tallest and stays there.
    #[test]
    fn size_is_held_across_frames() {
        let sprite = Sprite::parse("#: short\nTT\n#: tall\nTTTT\nTTTT\nTTTT\n");
        assert_eq!(sprite.width(), 4);
        assert_eq!(sprite.cell_height(), 2);
    }

    #[test]
    fn the_signal_ink_is_chosen_at_render_time() {
        let sprite = Sprite::parse("#: only\nSS\n");
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let danger = sprite.render_frame(palette, 0, Ink::Danger, 2);
        let good = sprite.render_frame(palette, 0, Ink::Good, 2);
        assert_ne!(danger[0].spans[0].style, good[0].spans[0].style);
    }

    /// The art is checked here rather than by eye: a feature that drifts half a cell
    /// out of alignment renders as a half block and reads as a smudge, which is
    /// invisible in a diff.
    #[test]
    fn the_mascot_has_two_eyes_and_a_beak_that_take_the_signal_colour() {
        let sprite = Sprite::parse(include_str!("../../assets/mascot.txt"));
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let signal = palette.color(Ink::Good);
        let rows = sprite.render_frame(palette, sprite.frame("idle"), Ink::Good, usize::MAX);

        let lit = |row: &ratatui::text::Line<'_>| {
            row.spans.iter().filter(|span| span.style.fg == Some(signal)).count()
        };
        let eyes = rows.iter().find(|row| lit(row) == 4).expect("one row holds both eyes");
        for column in [2, 3, 7, 8] {
            assert_eq!(eyes.spans[column].content.as_ref(), "\u{2588}", "eye {column} is not solid");
            assert_eq!(eyes.spans[column].style.fg, Some(signal), "eye {column}");
        }

        let beak = rows.iter().find(|row| lit(row) == 1).expect("one row holds the beak");
        assert_eq!(beak.spans[5].content.as_ref(), "\u{2588}", "the beak is not solid");
        assert_eq!(beak.spans[5].style.fg, Some(signal));
    }

    #[test]
    fn every_mascot_frame_is_the_same_size_and_keeps_its_beak() {
        let sprite = Sprite::parse(include_str!("../../assets/mascot.txt"));
        assert_eq!(sprite.frames(), 6);
        assert_eq!(sprite.width(), 11);
        assert_eq!(sprite.cell_height(), 5);

        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let signal = palette.color(Ink::Danger);
        for name in ["idle", "blink", "look-left", "look-right", "alert", "pleased"] {
            let rows = sprite.render_frame(palette, sprite.frame(name), Ink::Danger, usize::MAX);
            assert_eq!(rows.len(), 5, "{name} is a different height");
            let beak = &rows[2].spans[5];
            assert_eq!(beak.style.fg, Some(signal), "{name} lost its beak");
        }
    }

    #[test]
    fn a_partial_reveal_draws_only_the_columns_asked_for() {
        let sprite = Sprite::parse("#: only\nTTTT\n");
        let rendered = plain(&sprite.render_frame(Palette::with_depth(ColorDepth::Truecolor), 0, Ink::Signal, 2));
        assert_eq!(rendered, "\u{2580}\u{2580}  ");
    }

    #[test]
    fn ragged_rows_do_not_panic_and_pad_transparent() {
        let sprite = Sprite::parse("TTTT\nT");
        let rendered = plain(&sprite.render_frame(Palette::with_depth(ColorDepth::Truecolor), 0, Ink::Signal, usize::MAX));
        assert_eq!(rendered, "\u{2588}\u{2580}\u{2580}\u{2580}");
    }
}
