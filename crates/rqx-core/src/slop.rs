//! How much a file reads as written rather than read.
//!
//! No single signal here means much — a banner comment is a style, a narrated loop is
//! a habit, a placeholder is a hurry. Together and at density they describe code that
//! was produced and never read back, which is a different problem from code that is
//! merely bad, and needs a different response: read it, do not refactor it.
//!
//! This is a heuristic and says so everywhere it appears. It never gates unless a
//! project asks it to.

use crate::rule::RuleId;

/// What each signal is worth. Weighted rather than counted because a chat comment
/// says far more about how a file was made than a magic number does.
pub const SIGNALS: [(RuleId, u32); 15] = [
    (RuleId::ChatResidue, 10),
    (RuleId::ApologyComment, 6),
    (RuleId::BannerComment, 4),
    (RuleId::EmojiInSource, 4),
    (RuleId::ExampleValue, 5),
    (RuleId::RedundantComment, 5),
    (RuleId::JournalComment, 3),
    (RuleId::CommentedOutCode, 3),
    (RuleId::MeaninglessName, 2),
    (RuleId::VagueFunctionName, 2),
    (RuleId::EmptyCatch, 3),
    (RuleId::CommentPerLine, 6),
    (RuleId::ExplanationLongerThanCode, 4),
    (RuleId::DocstringRestatesSignature, 4),
    (RuleId::OverLogging, 3),
];

pub fn weight(rule: RuleId) -> u32 {
    SIGNALS
        .iter()
        .find(|(named, _)| *named == rule)
        .map_or(0, |(_, weight)| *weight)
}

/// Below this, density is not a measurement. A four-line file with a banner comment
/// has one signal per line and means nothing by it, so every file is scored as though
/// it were at least this long.
const FLOOR: u32 = 150;

/// One lapse is a lapse. The lightest single signal worth a score is a comment
/// addressed to a person, which is why this sits just under that weight — a lone
/// placeholder or banner is a finding on its own and nothing more.
const ENOUGH: u32 = 8;

/// The curve. Around one weighted signal per fifteen statements lands near 60, which
/// is where a reader should start looking rather than where a gate should fire.
pub fn score(weighted: u32, statements: u32) -> u32 {
    if weighted < ENOUGH {
        return 0;
    }
    let density = f64::from(weighted) / (f64::from(statements.max(FLOOR)) / 100.0);
    let score = 100.0 * (1.0 - (-density / 6.0).exp());
    score.round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A signal that is not a rule is a signal that never fires, and the compiler
    /// cannot catch that on its own.
    #[test]
    fn every_signal_is_a_rule_that_exists() {
        for (rule, weight) in SIGNALS {
            assert!(RuleId::ALL.contains(&rule), "{rule:?}");
            assert!(weight > 0, "{rule:?} is weightless");
        }
    }

    /// The family exists to feed this score, so a rule added to it and left out here
    /// would be collected and then ignored.
    #[test]
    fn every_noise_rule_is_a_signal() {
        for rule in RuleId::ALL {
            if rule.spec().category == crate::rule::Category::Noise {
                assert!(weight(*rule) > 0, "{} is not weighted", rule.slug());
            }
        }
    }

    #[test]
    fn the_score_rises_with_density_and_stops_at_a_hundred() {
        assert_eq!(score(0, 500), 0, "no signals is not a suspicion");
        assert_eq!(score(5, 40), 0, "one placeholder is one placeholder");
        assert!(
            score(10, 40) > 0,
            "one comment written to a person is a signal"
        );
        assert!(
            score(10, 1000) < score(10, 100),
            "same signals, denser file"
        );
        assert!(score(200, 100) <= 100);
        // A file of twenty statements with one chat comment is not evidence.
        assert!(score(10, 400) < 40, "{}", score(10, 400));
    }

    /// A tiny file would otherwise score high on one signal, and a one-line file with
    /// a banner comment is not what this is looking for.
    #[test]
    fn a_short_file_is_not_condemned_by_one_signal() {
        assert_eq!(score(4, 1), 0, "one banner in a four-line file");
        // But a file of the same length that is nothing but signals still is.
        assert!(score(60, 8) > 90, "{}", score(60, 8));
    }
}
