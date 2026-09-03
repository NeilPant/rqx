use std::env;

use ratatui::style::Color;

/// Every colour in Rqx is one of these. Screens name the role, never the hex, so
/// the palette can be retuned or downgraded in one place.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Ink {
    Bone,
    Amber,
    Terracotta,
    Moss,
    Sediment,
    Tar,
}

impl Ink {
    pub const fn from_sprite_char(character: char) -> Option<Self> {
        match character {
            'B' => Some(Self::Bone),
            'A' => Some(Self::Amber),
            'T' => Some(Self::Terracotta),
            'M' => Some(Self::Moss),
            'S' => Some(Self::Sediment),
            'K' => Some(Self::Tar),
            _ => None,
        }
    }

    const fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Bone => (0xE8, 0xE0, 0xD0),
            Self::Amber => (0xE8, 0xA3, 0x3D),
            Self::Terracotta => (0xC4, 0x56, 0x2E),
            Self::Moss => (0x6B, 0x8F, 0x5E),
            Self::Sediment => (0x3A, 0x41, 0x49),
            Self::Tar => (0x14, 0x17, 0x1A),
        }
    }

    const fn ansi256(self) -> u8 {
        match self {
            Self::Bone => 223,
            Self::Amber => 179,
            Self::Terracotta => 166,
            Self::Moss => 107,
            Self::Sediment => 238,
            Self::Tar => 234,
        }
    }

    const fn ansi16(self) -> Color {
        match self {
            Self::Bone => Color::White,
            Self::Amber => Color::Yellow,
            Self::Terracotta => Color::Red,
            Self::Moss => Color::Green,
            Self::Sediment => Color::DarkGray,
            Self::Tar => Color::Black,
        }
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
    /// `NO_COLOR` wins over every capability signal because it is a user's explicit
    /// instruction rather than a guess about the terminal.
    pub fn detect() -> Self {
        if env::var_os("NO_COLOR").is_some() {
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
}

pub const TAGLINE: &str = "every finding, a reason";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ink_resolves_at_every_depth() {
        let inks = [Ink::Bone, Ink::Amber, Ink::Terracotta, Ink::Moss, Ink::Sediment, Ink::Tar];
        for depth in [
            ColorDepth::Truecolor,
            ColorDepth::Ansi256,
            ColorDepth::Ansi16,
            ColorDepth::Monochrome,
        ] {
            let palette = Palette::with_depth(depth);
            for ink in inks {
                let color = palette.color(ink);
                if depth == ColorDepth::Monochrome {
                    assert_eq!(color, Color::Reset, "{ink:?} must not carry colour");
                }
            }
        }
    }

    #[test]
    fn sprite_legend_covers_every_ink() {
        for (character, ink) in [
            ('B', Ink::Bone),
            ('A', Ink::Amber),
            ('T', Ink::Terracotta),
            ('M', Ink::Moss),
            ('S', Ink::Sediment),
            ('K', Ink::Tar),
        ] {
            assert_eq!(Ink::from_sprite_char(character), Some(ink));
        }
        assert_eq!(Ink::from_sprite_char('.'), None, "`.` is transparent");
    }
}
