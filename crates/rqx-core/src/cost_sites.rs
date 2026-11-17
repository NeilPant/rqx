//! Places the cross-file stage of the cost analysis found for performance rules to
//! report. The stage knows which rule a place belongs to and what the evidence is;
//! this module says how each is worded, so a finding is built the same way wherever
//! the place came from.

use crate::cost::{Conf, Order, TraceStep};
use crate::finding::{Confidence, Evidence};
use crate::rule::RuleId;

/// The rules that read the composed cost of a function, and so cannot be answered from
/// one file alone.
pub const CROSS_FILE_RULES: [RuleId; 4] = [
    RuleId::QuadraticViaCall,
    RuleId::DeepRecursion,
    RuleId::RepeatedComputationInLoop,
    RuleId::PairScanCouldHash,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossSite {
    pub rule: RuleId,
    /// The file, as findings name it.
    pub file: String,
    /// Byte offset of the function the place is in, which is what its summary is found
    /// by.
    pub function: u32,
    pub line: u32,
    /// What the rule names: a callee, a function, a comparison.
    pub subject: String,
    /// How many loops enclose the place, inside its function.
    pub depth: u32,
    pub conf: Conf,
    /// What the evidence says, for the rules that say more than a name.
    pub message: String,
}

impl CrossSite {
    /// How sure the finding is: no more than the least sure thing the analysis used.
    pub const fn confidence(&self) -> Confidence {
        match self.conf {
            Conf::Verified => Confidence::Verified,
            Conf::Heuristic | Conf::Unknown => Confidence::Heuristic,
        }
    }

    pub fn evidence(&self) -> Evidence {
        match self.rule {
            RuleId::RepeatedComputationInLoop => Evidence::InLoop {
                subject: self.subject.clone(),
                depth: self.depth,
                sampled: None,
            },
            _ => Evidence::Name {
                identifier: self.subject.clone(),
                reason: self.message.clone(),
            },
        }
    }

    pub const fn fix_hint(&self) -> &'static str {
        match self.rule {
            RuleId::QuadraticViaCall => {
                "index what the callee searches once above the loop, or change the callee to look \
                 up by key so each pass costs constant time"
            }
            RuleId::DeepRecursion => {
                "walk the input with a loop and an explicit stack, so the depth is not limited by \
                 the call stack"
            }
            RuleId::RepeatedComputationInLoop => {
                "compute it once above the loop: nothing in the loop changes what it is given"
            }
            RuleId::PairScanCouldHash => {
                "build a map from the key of one collection above the loop and look each element \
                 of the other up in it"
            }
            _ => "",
        }
    }
}

/// The way to a function's cost, written as one line: the order, then each step, with
/// the file named only where it is not the one the line is about.
///
/// `O(n²): loop :90 → findMatch O(n) :12`
pub fn trace_text(order: &Order, steps: &[TraceStep], file: &str) -> String {
    let mut text = format!("{order}:");
    for (position, step) in steps.iter().enumerate() {
        text.push_str(if position == 0 { " " } else { " \u{2192} " });
        text.push_str(&step.what);
        if step.what != "loop" {
            text.push(' ');
            text.push_str(&step.order.to_string());
        }
        text.push(' ');
        if step.file != file {
            text.push_str(&step.file);
        }
        text.push_str(&format!(":{}", step.line));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(file: &str, line: u32, what: &str, order: &str) -> TraceStep {
        TraceStep {
            file: file.to_owned(),
            line,
            what: what.to_owned(),
            order: order.parse().unwrap(),
        }
    }

    #[test]
    fn a_trace_reads_from_the_loop_down_to_the_callee() {
        let steps = [
            step("a.ts", 90, "loop", "O(n²)"),
            step("a.ts", 12, "findMatch", "O(n)"),
        ];
        assert_eq!(
            trace_text(&"O(n²)".parse().unwrap(), &steps, "a.ts"),
            "O(n²): loop :90 \u{2192} findMatch O(n) :12"
        );
    }

    #[test]
    fn a_step_in_another_file_names_it() {
        let steps = [
            step("a.ts", 3, "loop", "O(n²)"),
            step("util/find.ts", 7, "find", "O(n)"),
        ];
        assert_eq!(
            trace_text(&"O(n²)".parse().unwrap(), &steps, "a.ts"),
            "O(n²): loop :3 \u{2192} find O(n) util/find.ts:7"
        );
    }

    #[test]
    fn a_heuristic_condition_is_a_heuristic_finding() {
        let site = CrossSite {
            rule: RuleId::QuadraticViaCall,
            file: "a.ts".to_owned(),
            function: 0,
            line: 1,
            subject: "find".to_owned(),
            depth: 1,
            conf: Conf::Unknown,
            message: String::new(),
        };
        assert_eq!(site.confidence(), Confidence::Heuristic);
    }
}
