//! `fixtures/complexity/baseline.json`: what the inference got right the last time it
//! was blessed. The ratchet reads it and fails when a function that was exact stops
//! being exact, or a metric drops; `RQX_BLESS=1` rewrites it on purpose. The format and
//! the metrics are `rqx_core::precision`, which the binary reads as well; this module
//! is the files they live in.

use std::path::Path;

pub use rqx_core::precision::{Baseline, Outcome, RuleCounts, RuleTable, Section};

pub fn read(path: &Path) -> Result<Baseline, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    Baseline::parse(&text)
}

pub fn write(baseline: &Baseline, path: &Path) -> Result<(), String> {
    std::fs::write(path, baseline.to_json()).map_err(|error| error.to_string())
}

/// The rules of the embedded table the binary reads, rewritten with the baseline so the
/// two cannot drift.
pub fn write_rule_table(baseline: &Baseline, path: &Path) -> Result<(), String> {
    std::fs::write(path, RuleTable::of(baseline).to_json()).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_baseline_is_written_to_a_file_and_read_back() {
        let path = std::env::temp_dir().join(format!("rqx-baseline-{}.json", std::process::id()));
        let baseline = Baseline::of([("a".to_owned(), Outcome::Exact)].into());
        write(&baseline, &path).unwrap();
        assert_eq!(read(&path).unwrap(), baseline);
        std::fs::remove_file(path).unwrap();
    }
}
