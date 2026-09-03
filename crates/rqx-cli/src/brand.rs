use std::env;

use ratatui::style::Color;

/// Every colour in Rqx is one of these. Screens name the role, never the hex, so
/// the palette can be retuned or downgraded in one place.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Ink {
    Accent,
    Danger,
    Warn,
    Good,
    Text,
    Muted,
    Ground,
}

impl Ink {
    #[cfg(test)]
    const ALL: [Self; 7] =
        [Self::Accent, Self::Danger, Self::Warn, Self::Good, Self::Text, Self::Muted, Self::Ground];

    pub const fn from_sprite_char(character: char) -> Option<Self> {
        match character {
            'A' => Some(Self::Accent),
            'D' => Some(Self::Danger),
            'W' => Some(Self::Warn),
            'G' => Some(Self::Good),
            'T' => Some(Self::Text),
            'M' => Some(Self::Muted),
            'K' => Some(Self::Ground),
            _ => None,
        }
    }

    const fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Accent => (0x4A, 0x9E, 0xFF),
            Self::Danger => (0xE5, 0x48, 0x4D),
            Self::Warn => (0xE8, 0xA3, 0x3D),
            Self::Good => (0x3D, 0xD6, 0x8C),
            Self::Text => (0xD6, 0xDE, 0xE8),
            Self::Muted => (0x5A, 0x64, 0x72),
            Self::Ground => (0x14, 0x17, 0x1A),
        }
    }

    const fn ansi256(self) -> u8 {
        match self {
            Self::Accent => 75,
            Self::Danger => 167,
            Self::Warn => 179,
            Self::Good => 78,
            Self::Text => 253,
            Self::Muted => 242,
            Self::Ground => 234,
        }
    }

    const fn ansi16(self) -> Color {
        match self {
            Self::Accent => Color::Blue,
            Self::Danger => Color::Red,
            Self::Warn => Color::Yellow,
            Self::Good => Color::Green,
            Self::Text => Color::White,
            Self::Muted => Color::DarkGray,
            Self::Ground => Color::Black,
        }
    }

    const fn ansi16_plain(self) -> anstyle::AnsiColor {
        match self {
            Self::Accent => anstyle::AnsiColor::Blue,
            Self::Danger => anstyle::AnsiColor::Red,
            Self::Warn => anstyle::AnsiColor::Yellow,
            Self::Good => anstyle::AnsiColor::Green,
            Self::Text => anstyle::AnsiColor::White,
            Self::Muted => anstyle::AnsiColor::BrightBlack,
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
        Self { depth: ColorDepth::detect() }
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

    pub fn dim(self) -> ratatui::style::Style {
        ratatui::style::Style::new().fg(self.color(Ink::Muted))
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

/// An empty style renders to nothing, so a monochrome palette costs no escape
/// sequences at all rather than emitting resets around every span.
pub fn paint(style: anstyle::Style, text: impl std::fmt::Display) -> String {
    format!("{}{text}{}", style.render(), style.render_reset())
}

pub const TAGLINE: &str = "every finding, a reason";

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
                    assert_ne!(palette.style(ink), anstyle::Style::new(), "{ink:?} lost its style");
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
        ] {
            assert_eq!(Ink::from_sprite_char(character), Some(ink));
        }
        assert_eq!(Ink::from_sprite_char('.'), None, "`.` is transparent");
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
