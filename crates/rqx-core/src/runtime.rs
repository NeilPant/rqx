use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::rule::RuleId;

mod speedscope;

/// Which lines of a file a test run actually executed. rqx decides dead code from the
/// import graph, which cannot see a dynamic import, a container wiring things up by
/// name, or reflection — so a symbol the graph called dead and a test ran is not a
/// finding rqx should keep insisting on.
#[derive(Debug, Default)]
pub struct Hits {
    by_file: BTreeMap<String, BTreeSet<u32>>,
}

impl Hits {
    pub fn is_empty(&self) -> bool {
        self.by_file.is_empty()
    }

    pub fn files(&self) -> usize {
        self.by_file.len()
    }

    /// Whether anything ran in the file at all. Paths are matched by suffix, because
    /// a coverage file records the path the test runner saw and rqx records the one
    /// the scan walked, and only one of them is relative to the project root.
    pub fn ran(&self, file: &str) -> bool {
        self.lines(file).is_some_and(|lines| !lines.is_empty())
    }

    /// How many of the lines in a range were executed.
    pub fn covered(&self, file: &str, from: u32, to: u32) -> u32 {
        self.lines(file)
            .map_or(0, |lines| lines.range(from..=to.max(from)).count() as u32)
    }

    fn lines(&self, file: &str) -> Option<&BTreeSet<u32>> {
        if let Some(lines) = self.by_file.get(file) {
            return Some(lines);
        }
        self.by_file
            .iter()
            .find(|(recorded, _)| recorded.ends_with(file) || file.ends_with(recorded.as_str()))
            .map(|(_, lines)| lines)
    }

    /// Two runs of the same suite, or one per language in a monorepo: a line either
    /// ran or it did not, so the union is the answer.
    #[must_use]
    pub fn merged(mut self, other: Self) -> Self {
        for (file, lines) in other.by_file {
            self.by_file.entry(file).or_default().extend(lines);
        }
        self
    }

    fn record(&mut self, file: &str, line: u32) {
        self.by_file
            .entry(normalise(file))
            .or_default()
            .insert(line);
    }

    /// The format is read from the file rather than declared, because a project
    /// naming its coverage file is not also going to want to name its dialect.
    pub fn read(path: &Path) -> Result<Self, String> {
        let body = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let mut hits = Self::default();

        if body.trim_start().starts_with('{') {
            hits.read_json(&body)?;
        } else if body
            .lines()
            .next()
            .is_some_and(|line| line.starts_with("mode:"))
        {
            hits.read_go(&body);
        } else if body.contains("SF:") {
            hits.read_lcov(&body);
        } else {
            return Err(format!(
                "{}: not an lcov file, a Go cover profile, or coverage.py JSON",
                path.display()
            ));
        }
        Ok(hits)
    }

    /// `SF:` opens a file, `DA:line,count` is one line, and a zero count means the
    /// line was instrumented and never reached.
    fn read_lcov(&mut self, body: &str) {
        let mut file = String::new();
        for line in body.lines() {
            if let Some(rest) = line.strip_prefix("SF:") {
                file = rest.trim().to_owned();
            } else if let Some(rest) = line.strip_prefix("DA:")
                && let Some((number, count)) = rest.split_once(',')
                && count.trim().parse::<u64>().is_ok_and(|count| count > 0)
                && let Ok(number) = number.trim().parse::<u32>()
            {
                self.record(&file, number);
            }
        }
    }

    /// `file:startLine.col,endLine.col statements count`, one region per line.
    fn read_go(&mut self, body: &str) {
        for line in body.lines().skip(1) {
            let Some((location, rest)) = line.split_once(' ') else {
                continue;
            };
            let Some((file, span)) = location.rsplit_once(':') else {
                continue;
            };
            let count: u64 = rest
                .split_whitespace()
                .nth(1)
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            if count == 0 {
                continue;
            }
            let Some((start, end)) = span.split_once(',') else {
                continue;
            };
            let parse = |part: &str| part.split('.').next()?.parse::<u32>().ok();
            if let (Some(first), Some(last)) = (parse(start), parse(end)) {
                for number in first..=last.max(first) {
                    self.record(file, number);
                }
            }
        }
    }

    /// coverage.py: `{"files": {"path": {"executed_lines": [...]}}}`.
    fn read_json(&mut self, body: &str) -> Result<(), String> {
        let value: serde_json::Value =
            serde_json::from_str(body).map_err(|error| error.to_string())?;
        let files = value
            .get("files")
            .and_then(serde_json::Value::as_object)
            .ok_or("coverage JSON with no `files` object")?;
        for (file, report) in files {
            let executed = report
                .get("executed_lines")
                .and_then(serde_json::Value::as_array);
            for line in executed.into_iter().flatten() {
                if let Some(number) = line.as_u64() {
                    self.record(file, number as u32);
                }
            }
        }
        Ok(())
    }
}

/// Coverage tools write the path they were given: absolute, `./`-prefixed, or
/// relative to a package. Only the shape is normalised here; the suffix match in
/// `lines` does the rest.
fn normalise(file: &str) -> String {
    file.trim_start_matches("./").replace('\\', "/")
}

/// The rules a test run can contradict, and what their confidence drops to when it
/// does. Only rules that decide from the import graph belong here: a run says
/// something about whether code was reached, and nothing about whether a function is
/// too long.
pub const CONTRADICTED: [(RuleId, crate::finding::Confidence); 2] = [
    (RuleId::UnusedExport, crate::finding::Confidence::Low),
    (RuleId::UnusedFile, crate::finding::Confidence::Low),
];

pub fn contradicted(rule: RuleId) -> Option<crate::finding::Confidence> {
    CONTRADICTED
        .iter()
        .find(|(named, _)| *named == rule)
        .map(|(_, confidence)| *confidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A profile says what ran. Letting it speak to a rule about naming or size
    /// would be reading a fact about time as a fact about design.
    #[test]
    fn only_performance_findings_answer_to_a_profile() {
        for rule in RuleId::ALL.iter().copied() {
            assert_eq!(
                unsampled(rule).is_some(),
                rule.spec().category == crate::rule::Category::Performance,
                "{rule}"
            );
        }
    }

    #[test]
    fn a_cpuprofile_weights_a_sample_by_its_delta_and_credits_the_stack() {
        let body = r#"{
            "nodes": [
                {"id": 1, "callFrame": {"functionName": "(root)", "url": "", "lineNumber": -1}, "children": [2]},
                {"id": 2, "callFrame": {"functionName": "outer", "url": "file:///w/a.ts", "lineNumber": 9}, "children": [3]},
                {"id": 3, "callFrame": {"functionName": "inner", "url": "file:///w/a.ts", "lineNumber": 19}}
            ],
            "samples": [3, 3, 2],
            "timeDeltas": [10, 10, 10]
        }"#;
        let samples = read_cpuprofile(body).expect("a cpuprofile");
        assert_eq!(samples.total(), 30);
        assert_eq!(
            samples.within("/w/a.ts", 20, 20),
            Some(Share {
                own: 666,
                total: 666
            }),
            "the leaf spent two of three samples"
        );
        assert_eq!(
            samples.within("/w/a.ts", 10, 10),
            Some(Share {
                own: 333,
                total: 1000
            }),
            "the caller was on the stack for all of them"
        );
    }

    #[test]
    fn collapsed_stacks_credit_a_repeated_frame_once() {
        let samples = read_collapsed("main;f;g 5\nmain;f 3\nmain;f;f 2\n");
        assert_eq!(samples.total(), 10);
        assert_eq!(
            samples.named("f").map(|share| share.total),
            Some(1000),
            "recursion must not credit `f` twice for one stack"
        );
        assert_eq!(samples.named("g").map(|share| share.own), Some(500));
    }

    #[test]
    fn a_py_spy_frame_carries_its_file_and_line() {
        let samples = read_collapsed("run (app/main.py:4);load (app/db.py:12) 7\n");
        assert_eq!(
            samples.within("app/db.py", 12, 12).map(|s| s.own),
            Some(1000)
        );
        assert_eq!(
            samples.within("app/main.py", 4, 4).map(|s| s.total),
            Some(1000)
        );
    }

    #[test]
    fn a_pprof_reads_the_names_and_lines_it_was_written_with() {
        let profile = {
            let mut out = Encoder::default();
            for text in ["", "sumCube", "app/hot.go"] {
                out.text(6, text);
            }
            out.message(
                5,
                &Encoder::of(|f| {
                    f.varint(1, 7);
                    f.varint(2, 1);
                    f.varint(4, 2);
                }),
            );
            out.message(
                4,
                &Encoder::of(|f| {
                    f.varint(1, 3);
                    f.message(
                        4,
                        &Encoder::of(|line| {
                            line.varint(1, 7);
                            line.varint(2, 42);
                        }),
                    );
                }),
            );
            out.message(
                2,
                &Encoder::of(|f| {
                    f.varint(1, 3);
                    f.varint(2, 9);
                }),
            );
            gzip(&out.bytes)
        };
        let samples = pprof::read(&profile).expect("a pprof");
        assert_eq!(samples.total(), 9);
        assert_eq!(
            samples.within("app/hot.go", 40, 45).map(|share| share.own),
            Some(1000)
        );
    }

    #[test]
    fn a_profile_of_no_known_shape_names_what_it_looked_for() {
        let path = std::env::temp_dir().join("rqx-not-a-profile.txt");
        std::fs::write(&path, "<xml/>").expect("write");
        let error = Samples::read(&path).expect_err("no format matches");
        assert!(error.contains(".cpuprofile"), "{error}");
        std::fs::remove_file(&path).ok();
    }

    /// Only the four wire pieces the reader parses, so a fixture profile can be
    /// written in the test rather than checked in as bytes nobody can read.
    #[derive(Default)]
    struct Encoder {
        bytes: Vec<u8>,
    }

    impl Encoder {
        fn of(fill: impl FnOnce(&mut Self)) -> Self {
            let mut out = Self::default();
            fill(&mut out);
            out
        }

        fn tag(&mut self, field: u64, wire: u64) {
            self.raw((field << 3) | wire);
        }

        fn raw(&mut self, mut value: u64) {
            loop {
                let byte = (value & 0x7f) as u8;
                value >>= 7;
                if value == 0 {
                    self.bytes.push(byte);
                    return;
                }
                self.bytes.push(byte | 0x80);
            }
        }

        fn varint(&mut self, field: u64, value: u64) {
            self.tag(field, 0);
            self.raw(value);
        }

        fn message(&mut self, field: u64, inner: &Self) {
            self.tag(field, 2);
            self.raw(inner.bytes.len() as u64);
            self.bytes.extend_from_slice(&inner.bytes);
        }

        fn text(&mut self, field: u64, value: &str) {
            self.tag(field, 2);
            self.raw(value.len() as u64);
            self.bytes.extend_from_slice(value.as_bytes());
        }
    }

    fn gzip(body: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut out = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        out.write_all(body).expect("gzip");
        out.finish().expect("gzip")
    }

    #[test]
    fn lcov_records_only_the_lines_that_ran() {
        let hits = {
            let mut hits = Hits::default();
            hits.read_lcov("SF:src/a.ts\nDA:1,4\nDA:2,0\nDA:7,1\nend_of_record\n");
            hits
        };
        assert!(hits.ran("src/a.ts"));
        assert_eq!(hits.covered("src/a.ts", 1, 10), 2, "line 2 never ran");
        assert!(!hits.ran("src/b.ts"));
    }

    #[test]
    fn a_go_profile_expands_a_region_into_its_lines() {
        let mut hits = Hits::default();
        hits.read_go("mode: set\nexample/a.go:3.10,6.2 2 1\nexample/a.go:9.2,11.4 1 0\n");
        assert_eq!(hits.covered("example/a.go", 3, 6), 4);
        assert_eq!(hits.covered("example/a.go", 9, 11), 0, "an unrun region");
    }

    #[test]
    fn coverage_json_reads_its_executed_lines() {
        let mut hits = Hits::default();
        hits.read_json(r#"{"files":{"pkg/a.py":{"executed_lines":[1,2,5]}}}"#)
            .expect("json");
        assert_eq!(hits.covered("pkg/a.py", 1, 5), 3);
    }

    /// A runner writes the path it was given and rqx walks its own, so the two agree
    /// on the end of the path and not the start.
    #[test]
    fn a_path_matches_by_its_end() {
        let mut hits = Hits::default();
        hits.read_lcov("SF:/build/checkout/src/a.ts\nDA:1,1\n");
        assert!(hits.ran("src/a.ts"));
        assert!(!hits.ran("other/a.ts"));
    }

    /// Only a rule the graph decided can be contradicted by a run: coverage says what
    /// was reached, never whether a function is too long.
    #[test]
    fn only_reachability_rules_can_be_contradicted() {
        for (rule, _) in CONTRADICTED {
            assert!(
                rule.is_project_scoped(),
                "{rule} is not decided by the graph"
            );
            assert_eq!(rule.spec().confidence, crate::finding::Confidence::Verified);
        }
        assert!(contradicted(RuleId::LongFunction).is_none());
    }
}

/// How much of a run's samples landed in one function: its own frames, and every
/// sample that had it anywhere on the stack. Parts per thousand, because a share is
/// read as a percentage and a float would make two runs of the same profile differ.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Share {
    pub own: u32,
    pub total: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct Weight {
    own: u64,
    total: u64,
}

impl Weight {
    fn share(self, of: u64) -> Share {
        let permille = |part: u64| part.saturating_mul(1000).checked_div(of).unwrap_or(0) as u32;
        Share {
            own: permille(self.own),
            total: permille(self.total),
        }
    }
}

/// What a CPU profile says ran. Static analysis names a shape that costs; a profile
/// says whether anything reached it, which is the difference between a quadratic loop
/// over three items and the one holding up a request.
#[derive(Debug, Default)]
pub struct Samples {
    by_line: BTreeMap<String, BTreeMap<u32, Weight>>,
    /// Frames a profile gives without a file: a JVM stack names `Class.method` and
    /// nothing else, so those are matched against function names instead.
    by_name: BTreeMap<String, Weight>,
    total: u64,
}

impl Samples {
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    /// The heaviest frame recorded inside a span. A profile records the line it
    /// sampled, which is somewhere in the body rather than at the signature.
    pub fn within(&self, file: &str, from: u32, to: u32) -> Option<Share> {
        let lines = self.lines(file)?;
        let mut found = Weight::default();
        let mut any = false;
        for (_, weight) in lines.range(from..=to.max(from)) {
            found.own += weight.own;
            found.total = found.total.max(weight.total);
            any = true;
        }
        any.then(|| found.share(self.total))
    }

    /// A frame that named no file. Ambiguity is not resolved here: the caller knows
    /// how many functions carry the name and drops the match when more than one does.
    pub fn named(&self, name: &str) -> Option<Share> {
        self.by_name
            .get(name)
            .map(|weight| weight.share(self.total))
    }

    /// Lines this profile recorded against files rqx never scanned, so a report can
    /// say how much of the run it could not place.
    pub fn frames(&self) -> usize {
        self.by_line.values().map(BTreeMap::len).sum::<usize>() + self.by_name.len()
    }

    /// Two profiles of the same service, or one per process: samples add up.
    #[must_use]
    pub fn merged(mut self, other: Self) -> Self {
        for (file, lines) in other.by_line {
            let target = self.by_line.entry(file).or_default();
            for (line, weight) in lines {
                let slot = target.entry(line).or_default();
                slot.own += weight.own;
                slot.total += weight.total;
            }
        }
        for (name, weight) in other.by_name {
            let slot = self.by_name.entry(name).or_default();
            slot.own += weight.own;
            slot.total += weight.total;
        }
        self.total += other.total;
        self
    }

    fn lines(&self, file: &str) -> Option<&BTreeMap<u32, Weight>> {
        if let Some(lines) = self.by_line.get(file) {
            return Some(lines);
        }
        self.by_line
            .iter()
            .find(|(recorded, _)| {
                recorded.ends_with(&format!("/{file}")) || file.ends_with(&format!("/{recorded}"))
            })
            .map(|(_, lines)| lines)
    }

    fn record(&mut self, frame: &Frame, own: u64, total: u64) {
        let slot = match (&frame.file, frame.line) {
            (Some(file), Some(line)) => self
                .by_line
                .entry(normalise(file))
                .or_default()
                .entry(line)
                .or_default(),
            _ => self.by_name.entry(frame.name.clone()).or_default(),
        };
        slot.own += own;
        slot.total += total;
    }

    /// The format is read from the file rather than declared, for the same reason
    /// coverage is: a project naming its profile is not going to name its dialect too.
    pub fn read(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if bytes.starts_with(&[0x1f, 0x8b]) {
            return pprof::read(&bytes).map_err(|error| format!("{}: {error}", path.display()));
        }
        let body = String::from_utf8(bytes)
            .map_err(|_| format!("{}: neither a pprof nor text", path.display()))?;
        if speedscope::claims(&body) {
            return speedscope::read(&body).map_err(|error| format!("{}: {error}", path.display()));
        }
        if body.trim_start().starts_with('{') && body.contains("\"nodes\"") {
            return read_cpuprofile(&body).map_err(|error| format!("{}: {error}", path.display()));
        }
        let stacks = read_collapsed(&body);
        if stacks.is_empty() {
            return Err(format!(
                "{}: not a V8 .cpuprofile, a speedscope file, a pprof, or collapsed stacks",
                path.display()
            ));
        }
        Ok(stacks)
    }
}

/// A profile placed against the functions rqx measured. Frames it could not place
/// are counted rather than guessed at, because a share attributed to the wrong
/// function is worse than no share at all.
#[derive(Debug)]
pub struct Profile {
    samples: Samples,
    matched: u32,
    unmatched: u32,
}

impl Profile {
    pub fn placed(samples: Samples, known: impl Fn(&str, u32) -> bool) -> Self {
        let mut matched = 0;
        let mut unmatched = 0;
        for (file, lines) in &samples.by_line {
            for line in lines.keys() {
                if known(file, *line) {
                    matched += 1;
                } else {
                    unmatched += 1;
                }
            }
        }
        unmatched += samples.by_name.len() as u32;
        Self {
            samples,
            matched,
            unmatched,
        }
    }

    pub fn of(&self, file: &str, from: u32, to: u32) -> Option<Share> {
        self.samples.within(file, from, to)
    }

    pub fn samples(&self) -> u64 {
        self.samples.total()
    }

    pub const fn matched(&self) -> u32 {
        self.matched
    }

    pub const fn unmatched(&self) -> u32 {
        self.unmatched
    }
}

/// One frame of a stack. A profile that names a file and a line can be placed
/// exactly; one that names only a symbol has to be matched against function names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Frame {
    name: String,
    file: Option<String>,
    line: Option<u32>,
}

impl Frame {
    fn key(&self) -> (Option<&str>, Option<u32>, &str) {
        (self.file.as_deref(), self.line, self.name.as_str())
    }
}

/// Credit a stack, leaf first: the leaf spent the time, and everything on the stack
/// was waiting for it. A frame repeated by recursion is credited once.
fn credit(samples: &mut Samples, stack: &[Frame], weight: u64) {
    if stack.is_empty() {
        return;
    }
    samples.total += weight;
    samples.record(&stack[0], weight, 0);
    let mut seen: Vec<(Option<&str>, Option<u32>, &str)> = Vec::with_capacity(stack.len());
    for frame in stack {
        if seen.contains(&frame.key()) {
            continue;
        }
        seen.push(frame.key());
        samples.record(frame, 0, weight);
    }
}

/// V8 and Chrome write a call tree plus the node each sample landed in. A sample
/// weighs the time delta recorded beside it, so a slow frame sampled once still
/// outweighs a fast one sampled twice.
fn read_cpuprofile(body: &str) -> Result<Samples, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|error| error.to_string())?;
    let nodes = value
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .ok_or("a .cpuprofile with no `nodes` array")?;

    let mut frames: BTreeMap<u64, Frame> = BTreeMap::new();
    let mut parents: BTreeMap<u64, u64> = BTreeMap::new();
    for node in nodes {
        let Some(id) = node.get("id").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        let call = node.get("callFrame");
        let name = call
            .and_then(|frame| frame.get("functionName"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let url = call
            .and_then(|frame| frame.get("url"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        // V8 counts lines from zero, and reports the line a function was declared on.
        let line = call
            .and_then(|frame| frame.get("lineNumber"))
            .and_then(serde_json::Value::as_i64)
            .filter(|line| *line >= 0)
            .map(|line| line as u32 + 1);
        frames.insert(
            id,
            Frame {
                name: name.to_owned(),
                file: (!url.is_empty()).then(|| strip_scheme(url)),
                line,
            },
        );
        for child in node
            .get("children")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_u64)
        {
            parents.insert(child, id);
        }
    }

    let taken = value
        .get("samples")
        .and_then(serde_json::Value::as_array)
        .ok_or("a .cpuprofile with no `samples` array")?;
    let deltas = value
        .get("timeDeltas")
        .and_then(serde_json::Value::as_array);

    let mut samples = Samples::default();
    for (index, node) in taken
        .iter()
        .filter_map(serde_json::Value::as_u64)
        .enumerate()
    {
        let weight = deltas
            .and_then(|deltas| deltas.get(index))
            .and_then(serde_json::Value::as_i64)
            .map_or(1, |delta| delta.unsigned_abs())
            .max(1);
        let mut stack = Vec::new();
        let mut current = Some(node);
        while let Some(id) = current {
            match frames.get(&id) {
                Some(frame) if !is_engine_frame(frame) => stack.push(frame.clone()),
                _ => {}
            }
            current = parents.get(&id).copied();
        }
        credit(&mut samples, &stack, weight);
    }
    Ok(samples)
}

/// `(root)`, `(program)`, `(idle)` and the collector are the engine's own frames.
/// They are not a function anybody wrote, so they are not a match nobody made.
fn is_engine_frame(frame: &Frame) -> bool {
    frame.file.is_none() && frame.name.starts_with('(')
}

fn strip_scheme(url: &str) -> String {
    url.trim_start_matches("file://")
        .trim_start_matches("webpack:///")
        .trim_start_matches("webpack://")
        .to_owned()
}

/// `frame;frame;frame count`, one stack per line: what py-spy, async-profiler and
/// perf all fold down to.
fn read_collapsed(body: &str) -> Samples {
    let mut samples = Samples::default();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((stack, count)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(weight) = count.trim().parse::<u64>() else {
            continue;
        };
        if !stack.contains(';') && stack.is_empty() {
            continue;
        }
        let frames: Vec<Frame> = stack.split(';').rev().map(parse_frame).collect();
        credit(&mut samples, &frames, weight);
    }
    samples
}

/// `name (file:line)` from py-spy, `pkg/Class.method_[j]` from async-profiler,
/// `symbol+0x1a` from perf.
fn parse_frame(text: &str) -> Frame {
    let text = text.trim();
    if let Some((name, rest)) = text.split_once(" (")
        && let Some(location) = rest.strip_suffix(')')
        && let Some((file, line)) = location.rsplit_once(':')
        && let Ok(line) = line.parse::<u32>()
    {
        return Frame {
            name: name.trim().to_owned(),
            file: Some(strip_scheme(file)),
            line: Some(line),
        };
    }
    let symbol = text
        .split_once("+0x")
        .map_or(text, |(head, _)| head)
        .trim_end_matches("_[j]")
        .trim_end_matches("_[i]")
        .trim_end_matches("_[k]");
    let dotted = symbol.rsplit('/').next().unwrap_or(symbol);
    Frame {
        name: dotted.to_owned(),
        file: None,
        line: None,
    }
}

/// The five messages of `profile.proto` a CPU profile needs, read by hand. A
/// generated decoder would pull in a code generator and a runtime for one file
/// format read once per run.
mod pprof {
    use std::collections::BTreeMap;

    use super::{Frame, Samples, credit};

    /// A location's own id, and the `(function id, line)` pair for each frame inlined
    /// into it — innermost first, which is the order pprof writes them in.
    type Located = (u64, Vec<(u64, u32)>);

    #[derive(Default)]
    struct Reader<'a> {
        bytes: &'a [u8],
        at: usize,
    }

    impl<'a> Reader<'a> {
        fn new(bytes: &'a [u8]) -> Self {
            Self { bytes, at: 0 }
        }

        fn done(&self) -> bool {
            self.at >= self.bytes.len()
        }

        fn varint(&mut self) -> Result<u64, &'static str> {
            let mut value = 0_u64;
            for shift in (0..64).step_by(7) {
                let byte = *self.bytes.get(self.at).ok_or("truncated varint")?;
                self.at += 1;
                value |= u64::from(byte & 0x7f) << shift;
                if byte & 0x80 == 0 {
                    return Ok(value);
                }
            }
            Err("varint longer than 64 bits")
        }

        fn bytes(&mut self) -> Result<&'a [u8], &'static str> {
            let length = self.varint()? as usize;
            let end = self.at.checked_add(length).ok_or("length overflows")?;
            let slice = self.bytes.get(self.at..end).ok_or("truncated field")?;
            self.at = end;
            Ok(slice)
        }

        /// Every field rqx does not read still has to be stepped over, and only its
        /// wire type says how far.
        fn skip(&mut self, wire: u64) -> Result<(), &'static str> {
            match wire {
                0 => {
                    self.varint()?;
                }
                1 => self.at += 8,
                2 => {
                    self.bytes()?;
                }
                5 => self.at += 4,
                _ => return Err("unknown wire type"),
            }
            Ok(())
        }

        fn field(&mut self) -> Result<(u64, u64), &'static str> {
            let tag = self.varint()?;
            Ok((tag >> 3, tag & 7))
        }

        fn packed(&mut self, wire: u64) -> Result<Vec<u64>, &'static str> {
            if wire == 2 {
                let mut inner = Reader::new(self.bytes()?);
                let mut values = Vec::new();
                while !inner.done() {
                    values.push(inner.varint()?);
                }
                return Ok(values);
            }
            Ok(vec![self.varint()?])
        }
    }

    #[derive(Default)]
    struct Function {
        name: u64,
        filename: u64,
    }

    pub(super) fn read(gzipped: &[u8]) -> Result<Samples, String> {
        let body = inflate(gzipped)?;
        let mut strings: Vec<String> = Vec::new();
        let mut functions: BTreeMap<u64, Function> = BTreeMap::new();
        let mut locations: BTreeMap<u64, Vec<(u64, u32)>> = BTreeMap::new();
        let mut stacks: Vec<(Vec<u64>, u64)> = Vec::new();

        let mut reader = Reader::new(&body);
        while !reader.done() {
            let (field, wire) = reader.field()?;
            match field {
                2 => stacks.push(sample(reader.bytes()?)?),
                4 => {
                    let (id, lines) = location(reader.bytes()?)?;
                    locations.insert(id, lines);
                }
                5 => {
                    let (id, function) = function(reader.bytes()?)?;
                    functions.insert(id, function);
                }
                6 => strings.push(String::from_utf8_lossy(reader.bytes()?).into_owned()),
                _ => reader.skip(wire)?,
            }
        }

        let text = |index: u64| strings.get(index as usize).cloned().unwrap_or_default();
        let mut samples = Samples::default();
        for (ids, weight) in stacks {
            let frames: Vec<Frame> = ids
                .iter()
                .filter_map(|id| locations.get(id))
                .flatten()
                .map(|(function, line)| {
                    let declared = functions.get(function);
                    let file = declared.map(|f| text(f.filename)).filter(|f| !f.is_empty());
                    Frame {
                        name: declared.map(|f| text(f.name)).unwrap_or_default(),
                        file,
                        line: Some(*line),
                    }
                })
                .collect();
            credit(&mut samples, &frames, weight);
        }
        Ok(samples)
    }

    /// `Sample { 1: location_id, 2: value }`, leaf location first, and the first
    /// value is the count a CPU profile was sampled with.
    fn sample(bytes: &[u8]) -> Result<(Vec<u64>, u64), &'static str> {
        let mut reader = Reader::new(bytes);
        let mut ids = Vec::new();
        let mut weight = 1;
        while !reader.done() {
            let (field, wire) = reader.field()?;
            match field {
                1 => ids = reader.packed(wire)?,
                2 => weight = reader.packed(wire)?.first().copied().unwrap_or(1),
                _ => reader.skip(wire)?,
            }
        }
        Ok((ids, weight.max(1)))
    }

    /// `Location { 1: id, 4: line }` with `Line { 1: function_id, 2: line }`.
    fn location(bytes: &[u8]) -> Result<Located, &'static str> {
        let mut reader = Reader::new(bytes);
        let mut id = 0;
        let mut lines = Vec::new();
        while !reader.done() {
            let (field, wire) = reader.field()?;
            match field {
                1 => id = reader.varint()?,
                4 => {
                    let mut inner = Reader::new(reader.bytes()?);
                    let mut function = 0;
                    let mut number = 0;
                    while !inner.done() {
                        let (field, wire) = inner.field()?;
                        match field {
                            1 => function = inner.varint()?,
                            2 => number = inner.varint()?,
                            _ => inner.skip(wire)?,
                        }
                    }
                    lines.push((function, number as u32));
                }
                _ => reader.skip(wire)?,
            }
        }
        Ok((id, lines))
    }

    /// `Function { 1: id, 2: name, 4: filename }`, both names being string indices.
    fn function(bytes: &[u8]) -> Result<(u64, Function), &'static str> {
        let mut reader = Reader::new(bytes);
        let mut id = 0;
        let mut function = Function::default();
        while !reader.done() {
            let (field, wire) = reader.field()?;
            match field {
                1 => id = reader.varint()?,
                2 => function.name = reader.varint()?,
                4 => function.filename = reader.varint()?,
                _ => reader.skip(wire)?,
            }
        }
        Ok((id, function))
    }

    fn inflate(gzipped: &[u8]) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let mut body = Vec::new();
        flate2::read::GzDecoder::new(gzipped)
            .read_to_end(&mut body)
            .map_err(|error| format!("not a readable pprof: {error}"))?;
        Ok(body)
    }
}

/// The family a run can speak to, and what its findings drop to when the run never
/// reached them. A profile says which code executed, which settles nothing about
/// whether a name is honest or a class too large.
pub fn unsampled(rule: RuleId) -> Option<crate::finding::Confidence> {
    (rule.spec().category == crate::rule::Category::Performance)
        .then_some(crate::finding::Confidence::Low)
}
