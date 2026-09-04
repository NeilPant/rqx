use crate::brand::Ink;

/// The mascot as it is drawn in the terminal. Read from the same file rather than
/// redrawn, so the owl on the page and the owl in `rqx` are one owl.
const ART: &str = include_str!("../../assets/mascot.txt");

/// Every component token resolves to one of the ten inks `brand.rs` declares, or to
/// a blend of two of them. Nothing on the page is a colour the terminal cannot
/// produce, which is why surfaces are tinted with `--ink-line` rather than lifted
/// toward white: white is not in the palette, and a grey card under a warm palette
/// is the drift that shows.
const COMPONENTS: &str = "  --background: var(--ink-ground);
  --foreground: var(--ink-text);
  --card: color-mix(in srgb, var(--ink-line) 9%, var(--ink-ground));
  --card-foreground: var(--ink-text);
  --popover: color-mix(in srgb, var(--ink-line) 14%, var(--ink-ground));
  --muted: color-mix(in srgb, var(--ink-line) 20%, var(--ink-ground));
  --muted-foreground: var(--ink-muted);
  --primary: var(--ink-accent);
  --primary-foreground: var(--ink-ground);
  --destructive: var(--ink-danger);
  --success: var(--ink-good);
  --warning: var(--ink-warn);
  --border: var(--ink-line);
  --border-soft: color-mix(in srgb, var(--ink-line) 45%, var(--ink-ground));
  --input: var(--ink-line);
  --ring: var(--ink-accent);
  --scrim: color-mix(in srgb, var(--ink-ground) 76%, transparent);
  --radius: 0.625rem;
";

/// The palette as CSS custom properties, taken from `brand.rs` rather than restated.
/// A colour retuned for the terminal moves the page with it.
pub fn variables() -> String {
    let mut out = String::from(":root {\n");
    for ink in Ink::EVERY {
        out.push_str(&format!("  --ink-{}: {};\n", ink.slug(), ink.hex()));
    }
    out.push('\n');
    out.push_str(COMPONENTS);
    out.push_str("}\n\n");

    // One class per ink, named by its slug, so the server can name the band and the
    // page never has to know which score wears which colour.
    for ink in Ink::EVERY {
        out.push_str(&format!(
            ".owl-{slug} {{\n\
             \x20 --owl-plumage: var(--ink-{slug});\n\
             \x20 --owl-shade: color-mix(in srgb, var(--ink-{slug}) 60%, black);\n\
             }}\n",
            slug = ink.slug()
        ));
    }
    out
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
        for ink in Ink::EVERY {
            assert!(css.contains(&format!(".owl-{}", ink.slug())), "{ink:?}");
        }
    }

    /// The terminal decides what the owl wears; the page only renders the answer. A
    /// band the server can name and the stylesheet has no class for is the drift.
    #[test]
    fn every_band_the_server_can_name_has_a_class() {
        let css = variables();
        for health in [
            None,
            Some(0),
            Some(49),
            Some(50),
            Some(79),
            Some(80),
            Some(100),
        ] {
            let slug = crate::brand::mascot_ink(health).slug();
            assert!(css.contains(&format!(".owl-{slug} ")), "health {health:?}");
        }
        assert_eq!(crate::brand::mascot_ink(Some(92)).slug(), "accent");
    }

    /// The page may only wear colours the terminal wears. A token given a literal of
    /// its own is how the two palettes drift apart without anyone noticing.
    #[test]
    fn no_colour_on_the_page_is_invented() {
        let brand: Vec<String> = Ink::EVERY.iter().map(|ink| ink.hex()).collect();
        let css = variables();
        let mut rest = css.as_str();
        while let Some(at) = rest.find('#') {
            let literal: String = rest[at..].chars().take(7).collect();
            assert!(
                brand.contains(&literal),
                "{literal} is not one of the ten inks"
            );
            rest = &rest[at + 7..];
        }

        for stylesheet in [include_str!("app/app.css"), include_str!("app/index.html")] {
            let strays: Vec<&str> = stylesheet
                .split('#')
                .skip(1)
                .filter(|after| {
                    after.len() >= 6 && after[..6].chars().all(|c| c.is_ascii_hexdigit())
                })
                .map(|after| &after[..6])
                .filter(|hex| !brand.iter().any(|ink| ink[1..].eq_ignore_ascii_case(hex)))
                .collect();
            assert!(strays.is_empty(), "colours outside the palette: {strays:?}");
        }
    }
}
