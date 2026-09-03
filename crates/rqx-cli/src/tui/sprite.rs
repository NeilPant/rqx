use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::brand::{Ink, Palette};

/// Pixel art is authored as text, one character per pixel, and embedded at compile
/// time: the art needs many passes to look right, and nobody iterates on a mascot
/// stored as a nested array of colour constants.
pub struct Sprite {
    rows: Vec<Vec<Option<Ink>>>,
    width: usize,
}

impl Sprite {
    pub fn parse(source: &str) -> Self {
        let rows: Vec<Vec<Option<Ink>>> = source
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| line.chars().map(Ink::from_sprite_char).collect())
            .collect();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        Self { rows, width }
    }

    pub const fn width(&self) -> u16 {
        self.width as u16
    }

    /// Two pixels share one cell vertically, so an odd-height sprite still needs a
    /// whole row for its last line.
    pub fn cell_height(&self) -> u16 {
        self.rows.len().div_ceil(2) as u16
    }

    fn pixel(&self, x: usize, y: usize) -> Option<Ink> {
        self.rows.get(y).and_then(|row| row.get(x).copied()).flatten()
    }

    /// Renders with half-blocks so each cell carries two vertically stacked pixels
    /// with independent colours; braille would double the resolution again but is
    /// monochrome per cell, which a coloured mascot cannot use.
    pub fn render(&self, palette: Palette) -> Vec<Line<'static>> {
        (0..self.rows.len())
            .step_by(2)
            .map(|y| {
                let spans = (0..self.width)
                    .map(|x| self.cell(x, y, palette))
                    .collect::<Vec<_>>();
                Line::from(spans)
            })
            .collect()
    }

    fn cell(&self, x: usize, y: usize, palette: Palette) -> Span<'static> {
        let top = self.pixel(x, y);
        let bottom = self.pixel(x, y + 1);

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
    use super::super::brand::ColorDepth;
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
        assert_eq!(sprite.width(), 2);
        assert_eq!(sprite.cell_height(), 2, "3 pixel rows need 2 cell rows");
    }

    #[test]
    fn glyph_encodes_which_of_the_two_pixels_are_set() {
        let sprite = Sprite::parse("T.T.\n.TT.");
        let rendered = plain(&sprite.render(Palette::with_depth(ColorDepth::Truecolor)));
        assert_eq!(rendered, "\u{2580}\u{2584}\u{2588} ");
    }

    /// Two different inks stacked in one cell are drawn in colour as a half-block
    /// with a background; with no colour available that would delete the lower
    /// pixel, so monochrome must fall back to a full block.
    #[test]
    fn no_pixel_is_lost_when_colour_is_unavailable() {
        let sprite = Sprite::parse("TA\nKB");
        assert_eq!(plain(&sprite.render(Palette::with_depth(ColorDepth::Monochrome))), "\u{2588}\u{2588}");
        assert_eq!(plain(&sprite.render(Palette::with_depth(ColorDepth::Truecolor))), "\u{2580}\u{2580}");
    }

    #[test]
    fn monochrome_still_distinguishes_the_four_pixel_states() {
        let sprite = Sprite::parse("T.T.\n.TT.");
        let mono = plain(&sprite.render(Palette::with_depth(ColorDepth::Monochrome)));
        assert_eq!(mono, "\u{2580}\u{2584}\u{2588} ");
    }

    #[test]
    fn comments_and_blank_lines_are_not_pixels() {
        let sprite = Sprite::parse("# rex, frame 1\n\nTT\n\nTT\n");
        assert_eq!(sprite.cell_height(), 1);
    }

    #[test]
    fn ragged_rows_do_not_panic_and_pad_transparent() {
        let sprite = Sprite::parse("TTTT\nT");
        assert_eq!(sprite.width(), 4);
        let rendered = plain(&sprite.render(Palette::with_depth(ColorDepth::Truecolor)));
        assert_eq!(rendered, "\u{2588}\u{2580}\u{2580}\u{2580}");
    }
}
