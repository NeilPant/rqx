//! speedscope's file format: one table of frames shared by every profile in the file,
//! and per profile either a list of sampled stacks or a list of open and close events.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{Frame, Samples, credit, is_engine_frame, strip_scheme};

/// A stack of indices into the shared frame table, root first.
type Stack = Vec<usize>;

pub(super) fn claims(body: &str) -> bool {
    let body = body.trim_start();
    body.starts_with('{')
        && (body.contains("speedscope")
            || (body.contains("\"shared\"")
                && body.contains("\"frames\"")
                && !body.contains("\"callFrame\"")))
}

pub(super) fn read(body: &str) -> Result<Samples, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("a speedscope profile that is not complete JSON: {error}"))?;
    let frames = frame_table(&value)?;
    let profiles = value
        .get("profiles")
        .and_then(Value::as_array)
        .ok_or("a speedscope file with no `profiles` array")?;

    let mut samples = Samples::default();
    for profile in profiles {
        let folded = match profile.get("type").and_then(Value::as_str) {
            Some("sampled") => fold_sampled(profile)?,
            Some("evented") => fold_evented(profile)?,
            other => return Err(format!("a speedscope profile of type {other:?}")),
        };
        for (stack, weight) in folded {
            let leaf_first: Vec<Frame> = stack
                .iter()
                .rev()
                .filter_map(|index| frames.get(*index))
                .filter(|frame| !is_engine_frame(frame))
                .cloned()
                .collect();
            credit(&mut samples, &leaf_first, weight);
        }
    }
    Ok(samples)
}

fn frame_table(value: &Value) -> Result<Vec<Frame>, String> {
    let frames = value
        .pointer("/shared/frames")
        .and_then(Value::as_array)
        .ok_or("a speedscope file with no `shared.frames` array")?;
    Ok(frames.iter().map(frame).collect())
}

fn frame(entry: &Value) -> Frame {
    let text = |key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    };
    Frame {
        name: text("name").unwrap_or_default().to_owned(),
        file: text("file").map(strip_scheme),
        line: entry
            .get("line")
            .and_then(Value::as_u64)
            .and_then(|line| u32::try_from(line).ok())
            .filter(|line| *line > 0),
    }
}

/// How many microseconds one unit of the profile's own measure stands for. Time is
/// credited in microseconds, which is what a V8 `.cpuprofile` uses, so the two kinds
/// of file weigh the same when merged. A profile measured in `none` or `bytes` counts
/// samples, and a unit is one.
fn microseconds_per_unit(profile: &Value) -> f64 {
    match profile.get("unit").and_then(Value::as_str) {
        Some("nanoseconds") => 0.001,
        Some("milliseconds") => 1_000.0,
        Some("seconds") => 1_000_000.0,
        _ => 1.0,
    }
}

/// A positive measure never rounds down to nothing: a sample that was taken counts.
fn weigh(value: f64, scale: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        ((value * scale).round() as u64).max(1)
    } else {
        0
    }
}

fn fold_sampled(profile: &Value) -> Result<BTreeMap<Stack, u64>, String> {
    let taken = profile
        .get("samples")
        .and_then(Value::as_array)
        .ok_or("a sampled speedscope profile with no `samples` array")?;
    let weights = profile.get("weights").and_then(Value::as_array);
    let scale = microseconds_per_unit(profile);

    let mut folded = BTreeMap::new();
    for (index, sample) in taken.iter().enumerate() {
        let weight = weights
            .and_then(|weights| weights.get(index))
            .and_then(Value::as_f64)
            .map_or(1, |weight| weigh(weight, scale));
        if weight > 0 {
            *folded.entry(indices(sample)).or_default() += weight;
        }
    }
    Ok(folded)
}

fn indices(stack: &Value) -> Stack {
    stack
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .filter_map(|index| usize::try_from(index).ok())
        .collect()
}

/// The interval between two events belongs to the stack that was open across it, so
/// each event first credits the interval that ended at it and then changes the stack.
/// Whatever is still open at the end of the profile runs to `endValue`.
fn fold_evented(profile: &Value) -> Result<BTreeMap<Stack, u64>, String> {
    let events = profile
        .get("events")
        .and_then(Value::as_array)
        .ok_or("an evented speedscope profile with no `events` array")?;
    let scale = microseconds_per_unit(profile);

    let mut folded: BTreeMap<Stack, u64> = BTreeMap::new();
    let mut open = Stack::new();
    let mut since = profile.get("startValue").and_then(Value::as_f64);
    let mut elapse = |open: &Stack, until: f64| {
        if let Some(from) = since
            && !open.is_empty()
        {
            let weight = weigh(until - from, scale);
            if weight > 0 {
                *folded.entry(open.clone()).or_default() += weight;
            }
        }
        since = Some(until);
    };

    for event in events {
        let Some(at) = event.get("at").and_then(Value::as_f64) else {
            continue;
        };
        elapse(&open, at);
        let Some(frame) = event
            .get("frame")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
        else {
            continue;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("O") => open.push(frame),
            Some("C") => {
                if let Some(position) = open.iter().rposition(|candidate| *candidate == frame) {
                    open.truncate(position);
                }
            }
            _ => {}
        }
    }
    if let Some(end) = profile.get("endValue").and_then(Value::as_f64) {
        elapse(&open, end);
    }
    Ok(folded)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::runtime::Share;

    const TYPESCRIPT: &str = "/fixtures/typescript/performance.ts";

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/profiles")
            .join(name)
    }

    fn share(samples: &Samples, line: u32) -> Option<Share> {
        samples.within(TYPESCRIPT, line, line)
    }

    #[test]
    fn a_sampled_profile_weighs_each_stack_by_its_weight() {
        let samples = Samples::read(&fixture("typescript.speedscope.json")).expect("speedscope");
        assert_eq!(samples.total(), 10_000, "the engine frame is not credited");
        assert_eq!(
            share(&samples, 18),
            Some(Share {
                own: 900,
                total: 900
            })
        );
        assert_eq!(share(&samples, 28), Some(Share { own: 0, total: 600 }));
        assert_eq!(
            share(&samples, 62),
            Some(Share {
                own: 100,
                total: 100
            })
        );
    }

    #[test]
    fn an_evented_profile_credits_the_stack_open_across_each_interval() {
        let samples =
            Samples::read(&fixture("typescript-evented.speedscope.json")).expect("speedscope");
        assert_eq!(samples.total(), 20_000);
        assert_eq!(
            share(&samples, 8),
            Some(Share {
                own: 300,
                total: 600
            })
        );
        assert_eq!(
            share(&samples, 18),
            Some(Share {
                own: 300,
                total: 300
            })
        );
        assert_eq!(
            share(&samples, 30),
            Some(Share {
                own: 400,
                total: 400
            })
        );
    }

    #[test]
    fn a_frame_still_open_at_the_end_runs_to_the_end_of_the_profile() {
        let body = r#"{"shared": {"frames": [{"name": "run", "file": "a.js", "line": 3}]},
            "profiles": [{"type": "evented", "unit": "none", "startValue": 0, "endValue": 9,
            "events": [{"type": "O", "frame": 0, "at": 1}]}]}"#;
        let samples = read(body).expect("speedscope");
        assert_eq!(samples.total(), 8);
    }

    #[test]
    fn a_close_for_a_frame_that_is_not_open_changes_nothing() {
        let body = r#"{"shared": {"frames": [{"name": "a", "file": "a.js", "line": 1},
            {"name": "b", "file": "a.js", "line": 2}]},
            "profiles": [{"type": "evented", "unit": "none", "startValue": 0, "endValue": 4,
            "events": [{"type": "O", "frame": 0, "at": 0}, {"type": "C", "frame": 1, "at": 2},
            {"type": "C", "frame": 0, "at": 4}]}]}"#;
        let samples = read(body).expect("speedscope");
        assert_eq!(
            samples.within("a.js", 1, 1).map(|share| share.own),
            Some(1000)
        );
    }

    #[test]
    fn units_are_converted_so_that_files_of_different_units_merge_fairly() {
        let sampled = |unit: &str, weight: u64| {
            format!(
                r#"{{"shared": {{"frames": [{{"name": "a", "file": "a.js", "line": 1}}]}},
                "profiles": [{{"type": "sampled", "unit": "{unit}", "samples": [[0]], "weights": [{weight}]}}]}}"#
            )
        };
        assert_eq!(
            read(&sampled("milliseconds", 2)).expect("ms").total(),
            2_000
        );
        assert_eq!(read(&sampled("nanoseconds", 2_000)).expect("ns").total(), 2);
        assert_eq!(read(&sampled("none", 7)).expect("none").total(), 7);
    }

    #[test]
    fn a_frame_index_outside_the_table_is_ignored() {
        let body = r#"{"shared": {"frames": [{"name": "a", "file": "a.js", "line": 1}]},
            "profiles": [{"type": "sampled", "samples": [[0, 5]]}]}"#;
        let samples = read(body).expect("speedscope");
        assert_eq!(
            samples.within("a.js", 1, 1).map(|share| share.own),
            Some(1000)
        );
    }

    #[test]
    fn a_file_with_shared_frames_is_recognised_without_a_schema() {
        let body = r#"{"shared": {"frames": []}, "profiles": []}"#;
        assert!(claims(body));
        assert!(!claims(
            r#"{"nodes": [{"callFrame": {}}], "shared": {"frames": []}}"#
        ));
    }

    #[test]
    fn a_truncated_file_is_an_error_that_names_the_format() {
        let error = Samples::read(&fixture("truncated.speedscope.json")).expect_err("truncated");
        assert!(error.contains("speedscope"), "{error}");
    }
}
