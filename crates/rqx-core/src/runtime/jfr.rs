//! Java frames: the text `jfr print --events jdk.ExecutionSample` writes, and the
//! `Class.method` names a JVM profiler leaves in collapsed stacks.
//!
//! A binary `.jfr` recording is not read. The format is versioned per JDK release and
//! holds far more than a CPU profile, so converting it with the JDK's own `jfr` tool is
//! the one route that stays correct as the format changes.

use super::{Frame, Samples, credit};

/// The first four bytes of every binary recording.
pub(super) const NATIVE_MAGIC: &[u8] = b"FLR\0";

pub(super) const NATIVE_RECORDING: &str = "a binary JFR recording, which rqx does not read; \
convert it with `jfr print --events jdk.ExecutionSample <file>` and pass the text it prints";

const EVENT: &str = "jdk.ExecutionSample {";
const STACK_OPEN: &str = "stackTrace = [";

pub(super) fn claims(body: &str) -> bool {
    body.contains(EVENT)
}

/// Every execution sample is one sample of weight one: a JFR sampler fires at a fixed
/// period, so a count is already proportional to time.
pub(super) fn read(body: &str) -> Result<Samples, String> {
    let mut reader = Reader::default();
    for line in body.lines() {
        reader.line(line.trim());
    }
    if reader.depth > 0 {
        return Err("a JFR text dump that ends inside an event".to_owned());
    }
    Ok(reader.samples)
}

/// Where in an event the dump is. `depth` counts open braces so that an object nested
/// in an event does not end it early.
#[derive(Default)]
struct Reader {
    samples: Samples,
    depth: usize,
    stack: Option<Vec<Frame>>,
}

impl Reader {
    fn line(&mut self, line: &str) {
        if let Some(stack) = &mut self.stack {
            if line == "]" {
                credit(&mut self.samples, stack, 1);
                self.stack = None;
            } else {
                stack.extend(stack_frame(line));
            }
        } else if self.depth == 0 {
            self.depth = usize::from(line.starts_with(EVENT));
        } else if line.starts_with(STACK_OPEN) && !line.ends_with(']') {
            self.stack = Some(Vec::new());
        } else if line.ends_with('{') {
            self.depth += 1;
        } else if line == "}" {
            self.depth -= 1;
        }
    }
}

/// `com.acme.Billing.reconcile(List) line: 88`, which is what the JDK prints, or
/// `com.acme.Billing.reconcile(Billing.java:88)`, which is how a Java stack trace
/// reads. A frame with no parentheses, such as the `...` that marks a stack cut at
/// the depth limit, is not a frame.
fn stack_frame(line: &str) -> Option<Frame> {
    let (qualified, rest) = line.split_once('(')?;
    let (inside, after) = rest.split_once(')')?;
    let (source, line) = match source_location(inside) {
        Some((source, line)) => (Some(source), Some(line)),
        None => (None, printed_line(after)),
    };
    Some(
        java_frame(qualified, line, source).unwrap_or_else(|| Frame {
            name: qualified.to_owned(),
            file: None,
            line: None,
        }),
    )
}

fn source_location(inside: &str) -> Option<(&str, u32)> {
    let (source, line) = inside.rsplit_once(':')?;
    Some((source, line.trim().parse().ok().filter(|line| *line > 0)?))
}

fn printed_line(after: &str) -> Option<u32> {
    let (_, rest) = after.split_once("line:")?;
    rest.trim_start()
        .split(|character: char| !character.is_ascii_digit())
        .next()?
        .parse()
        .ok()
        .filter(|line| *line > 0)
}

/// A frame of an async-profiler collapsed stack: `com/acme/Billing.reconcile_[j]`, or
/// `com.acme.Billing.reconcile:88_[j]` when the profiler was asked for line numbers.
pub(super) fn collapsed_frame(text: &str) -> Option<Frame> {
    let symbol = without_tier_marker(text.trim());
    let (qualified, line) = match symbol.rsplit_once(':') {
        Some((qualified, line)) => (qualified, Some(line.parse().ok()?)),
        None => (symbol, None),
    };
    java_frame(qualified, line, None)
}

/// async-profiler ends a frame with `_[j]` for Java, `_[i]` for inlined, `_[k]` for
/// the kernel, and `_[0]` or `_[1]` for the interpreter and C1.
fn without_tier_marker(symbol: &str) -> &str {
    symbol
        .strip_suffix(']')
        .and_then(|head| head.strip_suffix(|marker: char| marker.is_ascii_alphanumeric()))
        .and_then(|head| head.strip_suffix("_["))
        .unwrap_or(symbol)
}

/// A method named by its class, in either `com.acme.Billing` or `com/acme/Billing`
/// form. The class is what says which file the method is in, so a frame with no line
/// still names a file. A name that does not start a class with a capital is not Java:
/// `libjvm.so` and `start_thread` come through the same collapsed stacks.
fn java_frame(qualified: &str, line: Option<u32>, source: Option<&str>) -> Option<Frame> {
    let (class, method) = qualified.rsplit_once('.')?;
    let path = class.replace('.', "/");
    let (package, simple) = path.rsplit_once('/').unwrap_or(("", &path));
    if method.is_empty() || !simple.starts_with(|first: char| first.is_uppercase() || first == '$')
    {
        return None;
    }
    let outer = simple.split('$').find(|part| !part.is_empty())?;
    let file = source.map_or_else(|| format!("{outer}.java"), str::to_owned);
    Some(Frame {
        name: format!("{simple}.{method}"),
        file: Some(if package.is_empty() {
            file
        } else {
            format!("{package}/{file}")
        }),
        line,
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::runtime::Share;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/profiles")
            .join(name)
    }

    const PERFORMANCE: &str = "fixtures/java/src/main/java/rqx/Performance.java";

    #[test]
    fn an_execution_sample_is_placed_by_its_class_and_line() {
        let samples = Samples::read(&fixture("java.jfr.txt")).expect("jfr text");
        assert_eq!(samples.total(), 6);
        assert_eq!(
            samples.within(PERFORMANCE, 28, 28),
            Some(Share { own: 0, total: 500 }),
            "`contains` spent the time, and `keepWanted` was waiting for it"
        );
        assert_eq!(
            samples.within(PERFORMANCE, 40, 40),
            Some(Share {
                own: 333,
                total: 333
            }),
            "the file is found by the end of its path, from the class's package"
        );
        assert_eq!(
            samples.within(PERFORMANCE, 50, 50).map(|share| share.total),
            Some(166)
        );
    }

    #[test]
    fn collapsed_java_frames_are_matched_by_name_when_they_carry_no_line() {
        let samples = Samples::read(&fixture("java.collapsed.txt")).expect("collapsed");
        assert_eq!(samples.total(), 56);
        assert_eq!(
            samples
                .named("Performance.keepWanted")
                .map(|share| share.total),
            Some(535)
        );
        assert_eq!(
            samples.named("ArrayList.indexOf").map(|share| share.own),
            Some(535)
        );
    }

    #[test]
    fn collapsed_java_frames_with_line_numbers_are_placed_like_a_jfr_dump() {
        let samples = Samples::read(&fixture("java-lines.collapsed.txt")).expect("collapsed");
        assert_eq!(
            samples.within(PERFORMANCE, 28, 28),
            Some(Share { own: 0, total: 600 })
        );
        assert_eq!(
            samples.within(PERFORMANCE, 40, 40),
            Some(Share {
                own: 400,
                total: 400
            })
        );
    }

    #[test]
    fn the_printed_line_and_the_stack_trace_line_read_the_same() {
        let printed = stack_frame("com.acme.Billing.reconcile(List) line: 88  bci: 4").unwrap();
        let traced = stack_frame("com.acme.Billing.reconcile(Billing.java:88)").unwrap();
        assert_eq!(printed, traced);
        assert_eq!(printed.name, "Billing.reconcile");
        assert_eq!(printed.file.as_deref(), Some("com/acme/Billing.java"));
        assert_eq!(printed.line, Some(88));
    }

    #[test]
    fn a_nested_class_is_found_in_the_file_of_its_outer_class() {
        let frame = stack_frame("com.acme.Billing$Ledger.post(Object) line: 12").unwrap();
        assert_eq!(frame.name, "Billing$Ledger.post");
        assert_eq!(frame.file.as_deref(), Some("com/acme/Billing.java"));
    }

    #[test]
    fn a_stack_cut_at_the_depth_limit_ends_at_its_ellipsis() {
        let body =
            "jdk.ExecutionSample {\n  stackTrace = [\n    a.B.c() line: 3\n    ...\n  ]\n}\n";
        let samples = read(body).expect("jfr text");
        assert_eq!(samples.total(), 1);
        assert_eq!(samples.frames(), 1);
    }

    #[test]
    fn events_of_other_kinds_and_samples_without_a_stack_are_ignored() {
        let body = "jdk.GCHeapSummary {\n  heapSpace = {\n    start = 0x1\n  }\n}\n\
jdk.ExecutionSample {\n  stackTrace = null\n}\n";
        assert_eq!(read(body).expect("jfr text").total(), 0);
    }

    #[test]
    fn an_object_nested_in_a_sample_does_not_end_it() {
        let body = "jdk.ExecutionSample {\n  sampledThread = {\n    osName = \"main\"\n  }\n  \
stackTrace = [\n    a.B.c() line: 3\n  ]\n}\n";
        assert_eq!(read(body).expect("jfr text").total(), 1);
    }

    #[test]
    fn a_truncated_dump_is_an_error() {
        let error = Samples::read(&fixture("truncated.jfr.txt")).expect_err("truncated");
        assert!(error.contains("JFR"), "{error}");
    }

    #[test]
    fn a_binary_recording_is_refused_with_the_command_that_converts_it() {
        let path = std::env::temp_dir().join(format!("rqx-native-{}.jfr", std::process::id()));
        std::fs::write(&path, b"FLR\0\0\x02\0\x01rest of a recording").expect("write");
        let error = Samples::read(&path).expect_err("binary");
        std::fs::remove_file(&path).ok();
        assert!(
            error.contains("jfr print --events jdk.ExecutionSample"),
            "{error}"
        );
    }

    #[test]
    fn async_profiler_frames_read_in_slash_and_dot_form() {
        for text in [
            "com/acme/Billing.reconcile_[j]",
            "com.acme.Billing.reconcile_[0]",
            "com/acme/Billing.reconcile",
        ] {
            let frame = collapsed_frame(text).expect(text);
            assert_eq!(frame.name, "Billing.reconcile", "{text}");
            assert_eq!(
                frame.file.as_deref(),
                Some("com/acme/Billing.java"),
                "{text}"
            );
            assert_eq!(frame.line, None, "{text}");
        }
        let lined = collapsed_frame("com/acme/Billing.reconcile:88_[j]").expect("a line");
        assert_eq!(lined.line, Some(88));
    }

    #[test]
    fn frames_that_are_not_java_are_left_to_the_other_readers() {
        for text in [
            "libjvm.so",
            "start_thread",
            "[unknown]",
            "std::vector<int>::push_back",
        ] {
            assert_eq!(collapsed_frame(text), None, "{text}");
        }
    }
}
