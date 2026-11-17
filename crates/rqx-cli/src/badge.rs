//! A health badge as one self-contained SVG: a label on the left and the score and its
//! grade on the right, coloured by the grade's band.

use std::fmt::Write as _;

use rqx_core::Cleanliness;

use crate::brand::{Ink, grade_ink};

const LABEL: &str = "rqx";
const HEIGHT: u32 = 20;
const PADDING: u32 = 10;
/// Per character, for the 11 px sans-serif the text is drawn in. The text carries an
/// explicit length, so a renderer with a wider or narrower font still fits the box.
const CHARACTER_WIDTH: u32 = 7;
const FONT: &str = "Verdana,Geneva,DejaVu Sans,sans-serif";

pub fn svg(health: u32, findings: u32) -> String {
    let grade = Cleanliness::of(health, findings);
    let message = format!("health {health} {}", grade.label());
    let label_width = width_of(LABEL);
    let message_width = width_of(&message);
    let total = label_width + message_width;
    let (label_fill, message_fill) = (hex(Ink::Line), hex(grade_ink(grade)));
    let (label_ink, message_ink) = (hex(Ink::Text), hex(Ink::Ground));
    let description = format!("{LABEL}: {message}");

    let mut out = String::new();
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total}\" height=\"{HEIGHT}\" \
         role=\"img\" aria-label=\"{description}\">\n\
         <title>{description}</title>\n\
         <clipPath id=\"round\"><rect width=\"{total}\" height=\"{HEIGHT}\" rx=\"3\"/></clipPath>\n\
         <g clip-path=\"url(#round)\">\n\
         <rect width=\"{label_width}\" height=\"{HEIGHT}\" fill=\"{label_fill}\"/>\n\
         <rect x=\"{label_width}\" width=\"{message_width}\" height=\"{HEIGHT}\" fill=\"{message_fill}\"/>\n\
         </g>\n\
         <g text-anchor=\"middle\" font-family=\"{FONT}\" font-size=\"11\">\n"
    );
    write_text(&mut out, LABEL, label_width / 2, label_width, &label_ink);
    write_text(
        &mut out,
        &message,
        label_width + message_width / 2,
        message_width,
        &message_ink,
    );
    out.push_str("</g>\n</svg>\n");
    out
}

fn write_text(out: &mut String, text: &str, centre: u32, width: u32, fill: &str) {
    let _ = writeln!(
        out,
        "<text x=\"{centre}\" y=\"14\" fill=\"{fill}\" textLength=\"{}\" \
         lengthAdjust=\"spacing\">{text}</text>",
        width - PADDING
    );
}

fn width_of(text: &str) -> u32 {
    text.chars().count() as u32 * CHARACTER_WIDTH + PADDING
}

fn hex(ink: Ink) -> String {
    let (red, green, blue) = ink.rgb();
    format!("#{red:02X}{green:02X}{blue:02X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_badge_states_the_score_and_the_grade_in_the_grade_colour() {
        let badge = svg(82, 12);
        assert!(
            badge.contains("aria-label=\"rqx: health 82 fair\""),
            "{badge}"
        );
        assert!(badge.contains(">health 82 fair</text>"), "{badge}");
        assert!(
            badge.contains(&format!("fill=\"{}\"", hex(Ink::Accent))),
            "{badge}"
        );
    }

    #[test]
    fn each_band_has_its_own_colour() {
        let colours: Vec<String> = [(95, 3), (82, 3), (60, 3), (20, 3)]
            .iter()
            .map(|(health, findings)| hex(grade_ink(Cleanliness::of(*health, *findings))))
            .collect();
        assert_eq!(colours[0], hex(Ink::Good));
        assert_eq!(colours[1], hex(Ink::Accent));
        assert_eq!(colours[2], hex(Ink::Warn));
        assert_eq!(colours[3], hex(Ink::Danger));
    }

    #[test]
    fn a_project_with_no_findings_is_pristine() {
        assert!(svg(100, 0).contains("health 100 pristine"));
    }

    #[test]
    fn the_badge_refers_to_nothing_outside_itself() {
        let badge = svg(50, 9);
        assert!(!badge.contains("href"), "{badge}");
        assert!(!badge.contains("<image"), "{badge}");
        assert!(!badge.contains("<style"), "{badge}");
    }
}
