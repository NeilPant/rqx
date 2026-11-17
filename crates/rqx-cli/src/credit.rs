//! What a change paid off: the findings the base revision had that the change no
//! longer has, and how the health score moved.

use serde::Serialize;

use crate::scan::ScanOutcome;
use crate::verify::{self, Outcome};

#[derive(Clone, Debug, Serialize)]
pub struct Resolved {
    pub rule: String,
    pub subject: String,
    pub file: String,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub struct Credit {
    pub resolved: Vec<Resolved>,
    pub health_before: u32,
    pub health_after: u32,
}

impl Credit {
    /// `None` when either scan dropped findings to stay within its display cap: a
    /// finding the cap dropped from the working tree's list would read as fixed.
    pub fn between(before: &ScanOutcome, after: &ScanOutcome) -> Option<Self> {
        if before.omitted > 0 || after.omitted > 0 {
            return None;
        }
        let resolved = verify::changes_between(before, after)
            .into_iter()
            .filter(|change| matches!(change.outcome, Outcome::Resolved))
            .filter_map(|change| {
                let was = change.before?;
                Some(Resolved {
                    rule: change.slug,
                    subject: change.subject,
                    file: was.file.to_string(),
                    line: was.line,
                })
            })
            .collect();
        Some(Self {
            resolved,
            health_before: crate::report::Report::everything(before).health(),
            health_after: crate::report::Report::everything(after).health(),
        })
    }

    pub fn health_delta(&self) -> i32 {
        self.health_after as i32 - self.health_before as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_delta_is_signed() {
        let credit = Credit {
            resolved: Vec::new(),
            health_before: 80,
            health_after: 74,
        };
        assert_eq!(credit.health_delta(), -6);
    }
}
