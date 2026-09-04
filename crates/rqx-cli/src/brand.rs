use std::env;

use ratatui::style::Color;
use rqx_core::finding::Severity;
use rqx_core::{Cleanliness, Coverage};

/// Every colour in Rqx is one of these. Screens name the role, never the hex, so
/// the palette can be retuned or downgraded in one place.
///
/// The accent is the owl's own brown, which puts it inside the warm severity band rather
/// than beside it: tan and amber are separated by saturation and not by hue. Sixteen
/// colours have no brown, so `Accent` takes the yellow slot and `Warn` moves up to bright
/// yellow to stay distinguishable from it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Ink {
    Accent,
    Danger,
    Warn,
    Good,
    Text,
    Muted,
    Line,
    Ground,
    Signal,
    Shade,
}

impl Ink {
    #[cfg(test)]
    const ALL: [Self; 10] = [
        Self::Accent,
        Self::Danger,
        Self::Warn,
        Self::Good,
        Self::Text,
        Self::Muted,
        Self::Line,
        Self::Ground,
        Self::Signal,
        Self::Shade,
    ];

    pub const fn from_sprite_char(character: char) -> Option<Self> {
        match character {
            'A' => Some(Self::Accent),
            'D' => Some(Self::Danger),
            'W' => Some(Self::Warn),
            'G' => Some(Self::Good),
            'T' => Some(Self::Text),
            'M' => Some(Self::Muted),
            'K' => Some(Self::Ground),
            // Both resolved at render time rather than fixed in the art, so the mascot's
            // whole body can carry the health band without a frame per grade.
            'S' => Some(Self::Signal),
            'H' => Some(Self::Shade),
            _ => None,
        }
    }

    const fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Accent => (0xC8, 0x9B, 0x6A),
            Self::Danger => (0xE4, 0x57, 0x4A),
            Self::Warn => (0xE8, 0xA8, 0x3A),
            Self::Good => (0x8F, 0xB5, 0x5C),
            Self::Text => (0xF0, 0xE8, 0xDC),
            Self::Muted => (0x9E, 0x8F, 0x7C),
            Self::Line => (0x6E, 0x61, 0x50),
            Self::Ground => (0x0F, 0x0C, 0x09),
            Self::Signal => (0xC8, 0x9B, 0x6A),
            Self::Shade => (0x78, 0x5D, 0x3F),
        }
    }

    const fn ansi256(self) -> u8 {
        match self {
            Self::Accent | Self::Signal => 179,
            Self::Danger => 167,
            Self::Warn => 214,
            Self::Good => 107,
            Self::Text => 230,
            Self::Muted => 247,
            Self::Line => 242,
            Self::Ground => 233,
            Self::Shade => 137,
        }
    }

    /// One step down the 6x6x6 cube from the ink itself, so a shaded body stays the
    /// colour it is shading rather than turning grey.
    const fn shade_ansi256(self) -> u8 {
        match self {
            Self::Accent | Self::Signal | Self::Shade => 137,
            Self::Danger => 131,
            Self::Warn => 172,
            Self::Good => 65,
            Self::Text => 250,
            Self::Muted => 242,
            Self::Line | Self::Ground => 233,
        }
    }

    const fn ansi16(self) -> Color {
        match self {
            Self::Accent | Self::Signal | Self::Shade => Color::Yellow,
            Self::Danger => Color::Red,
            Self::Warn => Color::LightYellow,
            Self::Good => Color::Green,
            Self::Text => Color::White,
            Self::Muted | Self::Line => Color::DarkGray,
            Self::Ground => Color::Black,
        }
    }

    const fn ansi16_plain(self) -> anstyle::AnsiColor {
        match self {
            Self::Accent | Self::Signal | Self::Shade => anstyle::AnsiColor::Yellow,
            Self::Danger => anstyle::AnsiColor::Red,
            Self::Warn => anstyle::AnsiColor::BrightYellow,
            Self::Good => anstyle::AnsiColor::Green,
            Self::Text => anstyle::AnsiColor::White,
            Self::Muted | Self::Line => anstyle::AnsiColor::BrightBlack,
            Self::Ground => anstyle::AnsiColor::Black,
        }
    }
}

/// The health score bands live here rather than beside either renderer, because
/// the same score used to be green in `rqx scan` and amber in the interface.
pub const fn score_ink(score: u32) -> Ink {
    match score {
        80..=100 => Ink::Good,
        50..=79 => Ink::Warn,
        _ => Ink::Danger,
    }
}

pub const fn grade_ink(grade: Cleanliness) -> Ink {
    match grade {
        Cleanliness::Pristine | Cleanliness::Clean => Ink::Good,
        Cleanliness::Fair => Ink::Accent,
        Cleanliness::Poor => Ink::Warn,
        Cleanliness::Critical => Ink::Danger,
    }
}

/// Info is dim rather than accented: it is the severity a reader is meant to skim
/// past, and the interface used to paint it in the brand colour instead.
pub const fn severity_ink(severity: Severity) -> Ink {
    match severity {
        Severity::Error => Ink::Danger,
        Severity::Warning => Ink::Warn,
        Severity::Info => Ink::Muted,
    }
}

pub const fn coverage_ink<Answer: Copy>(coverage: Coverage<Answer>) -> Ink {
    match coverage {
        Coverage::Implemented(_) => Ink::Good,
        Coverage::Absent(_) => Ink::Warn,
        Coverage::NeedsTypes(_) | Coverage::NotStatic(_) | Coverage::NotApplicable(_) => Ink::Muted,
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    Truecolor,
    Ansi256,
    Ansi16,
    Monochrome,
}

impl ColorDepth {
    /// `--color` is resolved into anstream's global choice before anything draws, so
    /// reading it here keeps one answer for the interface and the plain reporter.
    /// `NO_COLOR` outranks every capability signal but not the flag, because a flag
    /// typed on the command line is the more recent of the two instructions.
    pub fn detect() -> Self {
        let forced = match anstream::ColorChoice::global() {
            anstream::ColorChoice::Never => return Self::Monochrome,
            anstream::ColorChoice::AlwaysAnsi => return Self::Ansi16,
            anstream::ColorChoice::Always => true,
            anstream::ColorChoice::Auto => false,
        };
        if !forced && env::var_os("NO_COLOR").is_some() {
            return Self::Monochrome;
        }
        let colorterm = env::var("COLORTERM").unwrap_or_default();
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            return Self::Truecolor;
        }
        let term = env::var("TERM").unwrap_or_default();
        if term.contains("256") {
            return Self::Ansi256;
        }
        Self::Ansi16
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Palette {
    depth: ColorDepth,
}

impl Palette {
    pub fn detect() -> Self {
        Self {
            depth: ColorDepth::detect(),
        }
    }

    pub const fn with_depth(depth: ColorDepth) -> Self {
        Self { depth }
    }

    pub const fn is_monochrome(self) -> bool {
        matches!(self.depth, ColorDepth::Monochrome)
    }

    pub fn color(self, ink: Ink) -> Color {
        match self.depth {
            ColorDepth::Truecolor => {
                let (r, g, b) = ink.rgb();
                Color::Rgb(r, g, b)
            }
            ColorDepth::Ansi256 => Color::Indexed(ink.ansi256()),
            ColorDepth::Ansi16 => ink.ansi16(),
            ColorDepth::Monochrome => Color::Reset,
        }
    }

    /// The mascot's two art inks, which both depend on what the scan found: `Signal` is
    /// the verdict colour and `Shade` is that same colour darkened, so the body keeps
    /// its form across a band running mint to coral.
    pub fn art_color(self, ink: Ink, signal: Ink) -> Color {
        match ink {
            Ink::Signal => self.color(signal),
            Ink::Shade => self.shade(signal),
            other => self.color(other),
        }
    }

    fn shade(self, ink: Ink) -> Color {
        match self.depth {
            ColorDepth::Truecolor => {
                let (r, g, b) = ink.rgb();
                Color::Rgb(darken(r), darken(g), darken(b))
            }
            ColorDepth::Ansi256 => Color::Indexed(ink.shade_ansi256()),
            // Sixteen colours hold nothing darker than a colour except black, and a black
            // underside punches a hole through the silhouette rather than shading it.
            ColorDepth::Ansi16 => ink.ansi16(),
            ColorDepth::Monochrome => Color::Reset,
        }
    }

    pub fn dim(self) -> ratatui::style::Style {
        ratatui::style::Style::new().fg(self.color(Ink::Muted))
    }

    /// Borders are their own ink rather than `Muted`, so a frame around a list does not
    /// carry the same weight as the dim text inside it.
    pub fn rule(self) -> ratatui::style::Style {
        ratatui::style::Style::new().fg(self.color(Ink::Line))
    }

    fn plain_color(self, ink: Ink) -> Option<anstyle::Color> {
        Some(match self.depth {
            ColorDepth::Truecolor => {
                let (r, g, b) = ink.rgb();
                anstyle::Color::Rgb(anstyle::RgbColor(r, g, b))
            }
            ColorDepth::Ansi256 => anstyle::Color::Ansi256(anstyle::Ansi256Color(ink.ansi256())),
            ColorDepth::Ansi16 => anstyle::Color::Ansi(ink.ansi16_plain()),
            ColorDepth::Monochrome => return None,
        })
    }

    /// The same palette for output that is not a ratatui frame. anstream already
    /// strips what it emits when the stream is not a terminal.
    pub fn style(self, ink: Ink) -> anstyle::Style {
        match self.plain_color(ink) {
            Some(color) => anstyle::Style::new().fg_color(Some(color)),
            None => anstyle::Style::new(),
        }
    }

    /// Marks a span inside a line of code. Reversed rather than tinted, because code
    /// on screen usually already carries the terminal's own colours.
    pub fn mark(self, ink: Ink) -> anstyle::Style {
        match self.plain_color(ink) {
            Some(color) => anstyle::Style::new()
                .bg_color(Some(color))
                .fg_color(self.plain_color(Ink::Ground)),
            None => anstyle::Style::new().underline(),
        }
    }

    /// Reversed rather than coloured, so a status word keeps its weight at every
    /// depth — with no colour at all the word alone still has to carry the verdict.
    pub fn badge(self, ink: Ink) -> anstyle::Style {
        match self.plain_color(ink) {
            Some(color) => anstyle::Style::new()
                .bg_color(Some(color))
                .fg_color(self.plain_color(Ink::Ground))
                .bold(),
            None => anstyle::Style::new().bold(),
        }
    }
}

const fn darken(channel: u8) -> u8 {
    (channel as u16 * 3 / 5) as u8
}

/// An empty style renders to nothing, so a monochrome palette costs no escape
/// sequences at all rather than emitting resets around every span.
pub fn paint(style: anstyle::Style, text: impl std::fmt::Display) -> String {
    format!("{}{text}{}", style.render(), style.render_reset())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ink_resolves_at_every_depth() {
        for depth in [
            ColorDepth::Truecolor,
            ColorDepth::Ansi256,
            ColorDepth::Ansi16,
            ColorDepth::Monochrome,
        ] {
            let palette = Palette::with_depth(depth);
            for ink in Ink::ALL {
                let color = palette.color(ink);
                if depth == ColorDepth::Monochrome {
                    assert_eq!(color, Color::Reset, "{ink:?} must not carry colour");
                    assert_eq!(palette.style(ink), anstyle::Style::new());
                } else {
                    assert_ne!(
                        palette.style(ink),
                        anstyle::Style::new(),
                        "{ink:?} lost its style"
                    );
                }
            }
        }
    }

    #[test]
    fn sprite_legend_covers_every_ink() {
        for (character, ink) in [
            ('A', Ink::Accent),
            ('D', Ink::Danger),
            ('W', Ink::Warn),
            ('G', Ink::Good),
            ('T', Ink::Text),
            ('M', Ink::Muted),
            ('K', Ink::Ground),
            ('S', Ink::Signal),
            ('H', Ink::Shade),
        ] {
            assert_eq!(Ink::from_sprite_char(character), Some(ink));
        }
        assert_eq!(Ink::from_sprite_char('.'), None, "`.` is transparent");
    }

    /// A shaded body has to stay the colour it is shading, or the mascot grows a grey
    /// belly on three of the four verdicts.
    #[test]
    fn the_shade_darkens_the_signal_without_leaving_its_hue() {
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        for signal in [Ink::Accent, Ink::Good, Ink::Warn, Ink::Danger] {
            let body = palette.art_color(Ink::Signal, signal);
            let shade = palette.art_color(Ink::Shade, signal);
            assert_ne!(body, shade, "{signal:?} shades to itself");
            let (Color::Rgb(br, bg, bb), Color::Rgb(sr, sg, sb)) = (body, shade) else {
                panic!("truecolor must resolve to rgb");
            };
            assert!(sr < br || br == 0, "{signal:?} red did not darken");
            assert!(sg < bg || bg == 0, "{signal:?} green did not darken");
            assert!(sb < bb || bb == 0, "{signal:?} blue did not darken");
        }
    }

    #[test]
    fn every_ink_that_carries_text_clears_the_contrast_floor() {
        for ink in [
            Ink::Text,
            Ink::Muted,
            Ink::Accent,
            Ink::Good,
            Ink::Warn,
            Ink::Danger,
        ] {
            let ratio = contrast(ink.rgb(), Ink::Ground.rgb());
            assert!(ratio >= 4.5, "{ink:?} is {ratio:.2}:1 on the ground");
        }
        let border = contrast(Ink::Line.rgb(), Ink::Ground.rgb());
        assert!(border >= 3.0, "a border at {border:.2}:1 is invisible");
    }

    fn contrast(one: (u8, u8, u8), other: (u8, u8, u8)) -> f64 {
        let lighter = luminance(one).max(luminance(other));
        let darker = luminance(one).min(luminance(other));
        (lighter + 0.05) / (darker + 0.05)
    }

    fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    #[test]
    fn the_score_bands_cover_every_score() {
        assert_eq!(score_ink(100), Ink::Good);
        assert_eq!(score_ink(80), Ink::Good);
        assert_eq!(score_ink(79), Ink::Warn);
        assert_eq!(score_ink(50), Ink::Warn);
        assert_eq!(score_ink(49), Ink::Danger);
        assert_eq!(score_ink(0), Ink::Danger);
    }
}
