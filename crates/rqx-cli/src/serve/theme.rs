use crate::brand::Ink;

/// The mascot as it is drawn in the terminal. Read from the same file rather than
/// redrawn, so the owl on the page and the owl in `rqx` are one owl.
const ART: &str = include_str!("../../assets/mascot.txt");

/// The palette as CSS custom properties, taken from `brand.rs` rather than restated.
/// A colour retuned for the terminal moves the page with it.
///
/// The names are the ones a component library uses — background, foreground, card,
/// primary, muted, border, ring — so the stylesheet reads like any other app's, with
/// rqx's own warm dark palette behind them instead of a default slate.
pub fn variables() -> String {
    let ink = |ink: Ink| ink.hex();
    let mut out = String::from(":root {\n");
    for value in Ink::EVERY {
        out.push_str(&format!("  --ink-{}: {};\n", value.slug(), ink(value)));
    }
    out.push_str(&format!(
        "\n  --background: {ground};\n\
         \x20 --foreground: {text};\n\
         \x20 --card: {card};\n\
         \x20 --card-foreground: {text};\n\
         \x20 --popover: {popover};\n\
         \x20 --muted: {muted_surface};\n\
         \x20 --muted-foreground: {muted};\n\
         \x20 --primary: {accent};\n\
         \x20 --primary-foreground: {ground};\n\
         \x20 --destructive: {danger};\n\
         \x20 --success: {good};\n\
         \x20 --warning: {warn};\n\
         \x20 --border: {border};\n\
         \x20 --input: {border};\n\
         \x20 --ring: {accent};\n\
         \x20 --radius: 0.625rem;\n\
         }}\n",
        ground = ink(Ink::Ground),
        text = ink(Ink::Text),
        // Lifted off the ground by a few points rather than given a colour of its
        // own: a card that is a different hue from the page reads as a dialog.
        card = lift(Ink::Ground, 0.045),
        popover = lift(Ink::Ground, 0.075),
        muted_surface = lift(Ink::Ground, 0.09),
        muted = ink(Ink::Muted),
        accent = ink(Ink::Accent),
        danger = ink(Ink::Danger),
        good = ink(Ink::Good),
        warn = ink(Ink::Warn),
        border = mix(Ink::Line, Ink::Ground, 0.55),
    ));
    out
}

/// One ink moved toward white, for the surfaces a flat palette has no colour for.
fn lift(ink: Ink, amount: f32) -> String {
    let (r, g, b) = rgb(ink);
    let raise = |channel: u8| (f32::from(channel) + (255.0 - f32::from(channel)) * amount) as u8;
    format!("#{:02x}{:02x}{:02x}", raise(r), raise(g), raise(b))
}

fn mix(ink: Ink, toward: Ink, amount: f32) -> String {
    let (r, g, b) = rgb(ink);
    let (tr, tg, tb) = rgb(toward);
    let blend =
        |from: u8, to: u8| (f32::from(from) + (f32::from(to) - f32::from(from)) * amount) as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        blend(r, tr),
        blend(g, tg),
        blend(b, tb)
    )
}

fn rgb(ink: Ink) -> (u8, u8, u8) {
    let hex = ink.hex();
    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0);
    (byte(1), byte(3), byte(5))
}

/// The owl, as one `<svg>` of 12x12 rects. `S` is left to a CSS variable so the
/// plumage follows the health score the way it does in the terminal: a failing
/// repository is a red owl rather than a brown one with a red detail on it.
pub fn owl(frame: &str) -> String {
    let rows = rows(frame);
    let mut cells = String::new();
    for (y, row) in rows.iter().enumerate() {
        for (x, character) in row.chars().enumerate() {
            let fill = match character {
                'S' => "var(--owl-plumage)",
                'H' => "var(--owl-shade)",
                'T' => "var(--ink-text)",
                'K' => "var(--ink-ground)",
                _ => continue,
            };
            cells.push_str(&format!(
                r#"<rect x="{x}" y="{y}" width="1" height="1" fill="{fill}"/>"#
            ));
        }
    }
    format!(
        r#"<svg class="owl" viewBox="0 0 12 12" shape-rendering="crispEdges" aria-hidden="true">{cells}</svg>"#
    )
}

fn rows(frame: &str) -> Vec<&str> {
    let marker = format!("#: {frame}");
    ART.lines()
        .skip_while(|line| line.trim() != marker)
        .skip(1)
        .take_while(|line| !line.starts_with('#') && !line.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owl_is_the_one_in_the_terminal() {
        let idle = rows("idle");
        assert_eq!(idle.len(), 12, "the mascot is twelve rows");
        assert!(idle.iter().all(|row| row.chars().count() == 12));

        let svg = owl("idle");
        assert!(svg.contains("var(--owl-plumage)"), "plumage is not themed");
        assert!(svg.contains("viewBox=\"0 0 12 12\""));
        // The eyes never follow the verdict, so they are the one part not a variable
        // the health score writes.
        assert!(svg.contains("var(--ink-text)"), "the iris is missing");
    }

    #[test]
    fn both_frames_are_readable() {
        assert_eq!(rows("blink").len(), 12);
    }

    /// The page takes the palette from `brand.rs`; restating a colour here is how the
    /// two would drift.
    #[test]
    fn every_ink_reaches_the_stylesheet() {
        let css = variables();
        for ink in Ink::EVERY {
            assert!(css.contains(&format!("--ink-{}", ink.slug())), "{ink:?}");
            assert!(css.contains(&ink.hex()), "{ink:?} is not its brand colour");
        }
        assert!(css.contains("--background"), "no component tokens");
        assert!(css.contains("--primary"));
    }
}
