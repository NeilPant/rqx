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
                frames.push(Frame {
                    name: name.trim().to_owned(),
                    rows: Vec::new(),
                });
                continue;
            }
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if frames.is_empty() {
                frames.push(Frame {
                    name: "idle".to_owned(),
                    rows: Vec::new(),
                });
            }
            let row = line.chars().map(Ink::from_sprite_char).collect();
            frames.last_mut().expect("a frame is open").rows.push(row);
        }
        if frames.is_empty() {
            frames.push(Frame {
                name: "idle".to_owned(),
                rows: Vec::new(),
            });
        }

        // Held across every frame so a layout constraint never jitters mid-animation.
        let width = frames
            .iter()
            .flat_map(|frame| frame.rows.iter().map(Vec::len))
            .max()
            .unwrap_or(0);
        let height = frames
            .iter()
            .map(|frame| frame.rows.len())
            .max()
            .unwrap_or(0);
        Self {
            frames,
            width,
            height,
        }
    }

    pub fn frame(&self, name: &str) -> usize {
        self.frames
            .iter()
            .position(|frame| frame.name == name)
            .unwrap_or(0)
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

    fn cell(
        &self,
        frame: usize,
        x: usize,
        y: usize,
        palette: Palette,
        signal: Ink,
    ) -> Span<'static> {
        let resolve = |ink: Ink| palette.art_color(ink, signal);
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

        // A solid cell is painted as a background rather than drawn as a full-block
        // glyph. Terminals fill a cell's background across the whole line box, but a
        // glyph only covers the height its font gives it — so with any line spacing a
        // grid of `\u{2588}` shows a gap between every row, and the art looks striped.
        let (glyph, style) = match (top, bottom) {
            (None, None) => (" ", Style::new()),
            (Some(color), None) => ("\u{2580}", Style::new().fg(color)),
            (None, Some(color)) => ("\u{2584}", Style::new().fg(color)),
            (Some(upper), Some(lower)) if upper == lower => (" ", Style::new().bg(upper)),
            (Some(upper), Some(lower)) => ("\u{2580}", Style::new().fg(upper).bg(lower)),
        };
        Span::styled(glyph, style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::ColorDepth;

    /// What a cell actually shows: a solid cell is a painted background, everything
    /// else is a glyph in a foreground colour.
    fn shown(span: &Span<'_>) -> Option<ratatui::style::Color> {
        if span.content.as_ref() == " " {
            span.style.bg
        } else {
            span.style.fg
        }
    }

    fn plain(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
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
        let rendered = plain(&sprite.render_frame(
            Palette::with_depth(ColorDepth::Truecolor),
            0,
            Ink::Signal,
            usize::MAX,
        ));
        assert_eq!(
            rendered, "\u{2580}\u{2584}  ",
            "a solid cell is a space with a background"
        );
    }

    /// Two different inks stacked in one cell are drawn in colour as a half-block
    /// with a background; with no colour available that would delete the lower
    /// pixel, so monochrome must fall back to a full block.
    #[test]
    fn no_pixel_is_lost_when_colour_is_unavailable() {
        let sprite = Sprite::parse("DW\nKT");
        assert_eq!(
            plain(&sprite.render_frame(
                Palette::with_depth(ColorDepth::Monochrome),
                0,
                Ink::Signal,
                usize::MAX
            )),
            "\u{2588}\u{2588}"
        );
        assert_eq!(
            plain(&sprite.render_frame(
                Palette::with_depth(ColorDepth::Truecolor),
                0,
                Ink::Signal,
                usize::MAX
            )),
            "\u{2580}\u{2580}"
        );
    }

    #[test]
    fn monochrome_still_distinguishes_the_four_pixel_states() {
        let sprite = Sprite::parse("T.T.\n.TT.");
        let mono = plain(&sprite.render_frame(
            Palette::with_depth(ColorDepth::Monochrome),
            0,
            Ink::Signal,
            usize::MAX,
        ));
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
        assert_eq!(
            sprite.frame("missing"),
            0,
            "an unknown name falls back to the first frame"
        );
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
        assert_ne!(shown(&danger[0].spans[0]), shown(&good[0].spans[0]));
    }

    const FRAMES: [&str; 4] = ["idle", "blink", "glance-left", "glance-right"];

    fn mascot() -> Sprite {
        Sprite::parse(include_str!("../../assets/mascot.txt"))
    }

    /// Pixel art cannot be judged by reading the file: every rejected version of this
    /// mascot looked fine as text and wrong on screen. Run it with
    /// `cargo test -p rqx render_mascot -- --nocapture`.
    #[test]
    fn render_mascot() {
        let sprite = mascot();
        for depth in [
            ColorDepth::Truecolor,
            ColorDepth::Ansi256,
            ColorDepth::Ansi16,
        ] {
            for signal in [Ink::Accent, Ink::Good, Ink::Warn, Ink::Danger] {
                println!("\n{depth:?} / {signal:?}");
                let drawn: Vec<Vec<String>> = FRAMES
                    .iter()
                    .map(|name| {
                        let rows = sprite.render_frame(
                            Palette::with_depth(depth),
                            sprite.frame(name),
                            signal,
                            usize::MAX,
                        );
                        rows.iter().map(escape).collect()
                    })
                    .collect();
                for row in 0..sprite.cell_height() as usize {
                    let line: Vec<&str> = drawn.iter().map(|frame| frame[row].as_str()).collect();
                    println!("{}", line.join("  "));
                }
            }
        }
        println!("\nMonochrome");
        for row in mascot().render_frame(
            Palette::with_depth(ColorDepth::Monochrome),
            0,
            Ink::Accent,
            usize::MAX,
        ) {
            println!("{}", plain(std::slice::from_ref(&row)));
        }
        for name in FRAMES {
            println!("{name}");
        }
    }

    /// The header owl, which is small enough that a pixel out of place changes the face.
    /// Run it with `cargo test -p rqx render_compact_owl -- --nocapture`.
    #[test]
    fn render_compact_owl() {
        let sprite = Sprite::parse(include_str!("../../assets/mascot-compact.txt"));
        for depth in [ColorDepth::Truecolor, ColorDepth::Monochrome] {
            println!("\n{depth:?}");
            let drawn: Vec<Vec<String>> = FRAMES
                .iter()
                .map(|name| {
                    sprite
                        .render_frame(
                            Palette::with_depth(depth),
                            sprite.frame(name),
                            Ink::Good,
                            usize::MAX,
                        )
                        .iter()
                        .map(escape)
                        .collect()
                })
                .collect();
            for row in 0..sprite.cell_height() as usize {
                let line: Vec<&str> = drawn.iter().map(|frame| frame[row].as_str()).collect();
                println!("{}", line.join("  "));
            }
        }
    }

    fn escape(line: &Line<'_>) -> String {
        let mut out = String::new();
        for span in &line.spans {
            if let Some(color) = span.style.fg {
                out.push_str(&sgr(color, 3));
            }
            if let Some(color) = span.style.bg {
                out.push_str(&sgr(color, 4));
            }
            out.push_str(span.content.as_ref());
            out.push_str("\u{1b}[0m");
        }
        out
    }

    fn sgr(color: Color, layer: u8) -> String {
        match color {
            Color::Rgb(r, g, b) => format!("\u{1b}[{layer}8;2;{r};{g};{b}m"),
            Color::Indexed(index) => format!("\u{1b}[{layer}8;5;{index}m"),
            Color::Black => format!("\u{1b}[{}m", layer * 10),
            Color::Red => format!("\u{1b}[{}m", layer * 10 + 1),
            Color::Green => format!("\u{1b}[{}m", layer * 10 + 2),
            Color::Yellow => format!("\u{1b}[{}m", layer * 10 + 3),
            Color::Cyan => format!("\u{1b}[{}m", layer * 10 + 6),
            Color::White => format!("\u{1b}[{}m", layer * 10 + 7),
            Color::DarkGray => format!("\u{1b}[{}m", layer * 10 + 60),
            _ => String::new(),
        }
    }

    /// The art is checked here rather than by eye: a feature that drifts half a cell out
    /// of alignment renders as a half block and reads as a smudge, and a solid cell that
    /// is drawn as a glyph rather than painted leaves a gap on any terminal with line
    /// spacing. Neither is visible in a diff.
    ///
    /// Cell rows 2 and 3 are the eyes, cell row 3 also carries the beak, and cell row 5
    /// is the chest.
    #[test]
    fn the_mascot_is_an_owl_with_two_eyes_and_a_beak() {
        let sprite = mascot();
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let rows = sprite.render_frame(palette, sprite.frame("idle"), Ink::Good, usize::MAX);
        let iris = palette.color(Ink::Text);
        let pupil = palette.color(Ink::Ground);

        // Each iris is four columns wide, and the two inner ones hold the pupil across the
        // boundary between the two cell rows the eye spans.
        for column in [1, 4, 7, 10] {
            let span = &rows[2].spans[column];
            assert_eq!(
                span.content.as_ref(),
                " ",
                "iris cell 2,{column} is not solid"
            );
            assert_eq!(span.style.bg, Some(iris), "iris cell 2,{column}");
        }
        for column in [2, 3, 8, 9] {
            assert_eq!(
                rows[2].spans[column].style.bg,
                Some(pupil),
                "pupil top {column}"
            );
            assert_eq!(
                rows[3].spans[column].style.fg,
                Some(pupil),
                "pupil bottom {column}"
            );
        }

        let beak = palette.art_color(Ink::Shade, Ink::Good);
        for column in [5, 6] {
            let span = &rows[3].spans[column];
            assert_eq!(
                span.content.as_ref(),
                " ",
                "beak cell {column} is not solid"
            );
            assert_eq!(span.style.bg, Some(beak), "beak cell {column}");
        }
    }

    /// The tufts are the only thing above the head, so they are what the top cell row is.
    #[test]
    fn the_owl_has_two_ear_tufts_and_nothing_between_them() {
        let sprite = mascot();
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let rows = sprite.render_frame(palette, sprite.frame("idle"), Ink::Good, usize::MAX);
        let drawn = |column: usize| shown(&rows[0].spans[column]).is_some();
        assert!(drawn(1) && drawn(2), "the left tuft is missing");
        assert!(drawn(9) && drawn(10), "the right tuft is missing");
        for column in 4..8 {
            assert!(
                !drawn(column),
                "column {column} joins the tufts into one crown"
            );
        }
    }

    #[test]
    fn the_blink_is_the_only_difference_between_idle_and_blink() {
        let sprite = mascot();
        assert_eq!(sprite.frames(), FRAMES.len());
        assert_eq!(sprite.width(), 12);
        assert_eq!(sprite.cell_height(), 6);

        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let iris = palette.color(Ink::Text);
        let drawn =
            |name: &str| sprite.render_frame(palette, sprite.frame(name), Ink::Danger, usize::MAX);
        let (open, shut) = (drawn("idle"), drawn("blink"));
        assert_eq!(open.len(), 6);
        assert_eq!(shut.len(), 6);

        let irises = |rows: &[Line<'_>]| {
            rows.iter()
                .flat_map(|row| row.spans.iter())
                .filter(|span| span.style.fg == Some(iris) || span.style.bg == Some(iris))
                .count()
        };
        assert_eq!(irises(&open), 16, "an open owl shows two four-column eyes");
        assert_eq!(irises(&shut), 0, "a blinking owl shows no iris at all");

        // Only the eyes move. Anything else changing means the owl shifted rather than blinked.
        for row in [0, 1, 4, 5] {
            assert_eq!(
                plain(&open[row..=row]),
                plain(&shut[row..=row]),
                "cell row {row} moved during the blink"
            );
        }
    }

    /// A glance moves the pupils inside irises that stay where they were: any other cell
    /// changing would mean the face moved rather than the eyes.
    #[test]
    fn a_glance_moves_only_the_pupils() {
        let sprite = mascot();
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let drawn =
            |name: &str| sprite.render_frame(palette, sprite.frame(name), Ink::Danger, usize::MAX);
        let idle = drawn("idle");
        let pupil = palette.color(Ink::Ground);
        let pupils = |rows: &[Line<'_>]| -> Vec<usize> {
            rows[2]
                .spans
                .iter()
                .enumerate()
                .filter(|(_, span)| span.style.bg == Some(pupil))
                .map(|(column, _)| column)
                .collect()
        };
        assert_eq!(pupils(&idle), [2, 3, 8, 9]);
        for (name, columns) in [
            ("glance-left", [1, 2, 7, 8]),
            ("glance-right", [3, 4, 9, 10]),
        ] {
            let looking = drawn(name);
            assert_eq!(pupils(&looking), columns, "{name}");
            for row in [0, 1, 4, 5] {
                assert_eq!(
                    plain(&idle[row..=row]),
                    plain(&looking[row..=row]),
                    "{name} moved cell row {row}"
                );
            }
        }
    }

    /// The eyes are the one part that does not follow the verdict, so the face is the same
    /// face on a failing repository as on a clean one.
    #[test]
    fn the_eyes_hold_their_colour_while_the_plumage_follows_the_verdict() {
        let sprite = mascot();
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let cell = |signal: Ink, row: usize, column: usize| {
            let rows = sprite.render_frame(palette, sprite.frame("idle"), signal, usize::MAX);
            shown(&rows[row].spans[column])
        };
        assert_eq!(cell(Ink::Good, 2, 1), Some(palette.color(Ink::Text)));
        assert_eq!(cell(Ink::Danger, 2, 1), Some(palette.color(Ink::Text)));
        assert_ne!(
            cell(Ink::Accent, 1, 5),
            cell(Ink::Danger, 1, 5),
            "the plumage ignored the verdict"
        );
    }

    /// The plumage and its shade are two colours on every frame, or the art is flat. Both
    /// halves of a cell count: a shade drawn as the background of a half block is as much
    /// on screen as one painted solid.
    #[test]
    fn the_body_carries_a_shade_as_well_as_the_verdict_colour() {
        let sprite = mascot();
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        for name in FRAMES {
            let rows = sprite.render_frame(palette, sprite.frame(name), Ink::Warn, usize::MAX);
            let body = palette.art_color(Ink::Signal, Ink::Warn);
            let shade = palette.art_color(Ink::Shade, Ink::Warn);
            for (part, wanted) in [("plumage", body), ("shade", shade)] {
                let count = rows
                    .iter()
                    .flat_map(|row| row.spans.iter())
                    .filter(|span| span.style.fg == Some(wanted) || span.style.bg == Some(wanted))
                    .count();
                assert!(count > 6, "{name} barely uses its {part}");
            }
        }
    }

    /// Prints the wipe frame by frame, in the colour a failing scan lands in. Run it with
    /// `cargo test -p rqx render_wipe -- --nocapture`.
    #[test]
    fn render_wipe() {
        let sprite = mascot();
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        for step in 0..=sprite.width() as usize {
            println!("\nreveal {step}");
            for line in sprite.render_frame(palette, sprite.frame("idle"), Ink::Danger, step) {
                println!("{}", escape(&line));
            }
        }
    }

    #[test]
    fn a_partial_reveal_draws_only_the_columns_asked_for() {
        let sprite = Sprite::parse("#: only\nTTTT\n");
        let rendered = plain(&sprite.render_frame(
            Palette::with_depth(ColorDepth::Truecolor),
            0,
            Ink::Signal,
            2,
        ));
        assert_eq!(rendered, "\u{2580}\u{2580}  ");
    }

    #[test]
    fn ragged_rows_do_not_panic_and_pad_transparent() {
        let sprite = Sprite::parse("TTTT\nT");
        let rendered = plain(&sprite.render_frame(
            Palette::with_depth(ColorDepth::Truecolor),
            0,
            Ink::Signal,
            usize::MAX,
        ));
        assert_eq!(rendered, " \u{2580}\u{2580}\u{2580}");
    }
}
