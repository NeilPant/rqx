//! Reads and writes the `.expect` format described in the parent module.

use rqx_core::RuleId;

use super::BLESS_COMMAND;

const TEXT_SEPARATOR: &str = " | ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Fires,
    Silent,
    Either,
    KnownFalsePositive { reason: String },
}

impl Verdict {
    fn keyword(&self) -> &'static str {
        match self {
            Self::Fires => "rule",
            Self::Silent => "none",
            Self::Either => "may",
            Self::KnownFalsePositive { .. } => "known-fp",
        }
    }

    fn counts_repeats(&self) -> bool {
        matches!(self, Self::Fires | Self::KnownFalsePositive { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expectation {
    pub file: String,
    pub rule: String,
    pub verdict: Verdict,
    /// How many times the rule fires on the line.
    pub repeats: u32,
    /// The trimmed source line.
    pub text: String,
    /// Which of the file's lines with this text is meant, counting from one.
    pub occurrence: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Problem {
    pub line: usize,
    pub message: String,
}

/// Every entry in the file, and every line that is not one.
pub fn parse(text: &str) -> (Vec<Expectation>, Vec<Problem>) {
    let mut expectations = Vec::new();
    let mut problems = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_entry(line) {
            Ok(expectation) => expectations.push(expectation),
            Err(message) => problems.push(Problem {
                line: index + 1,
                message,
            }),
        }
    }
    (expectations, problems)
}

fn parse_entry(line: &str) -> Result<Expectation, String> {
    let (head, text) = split_source_text(line)
        .ok_or_else(|| format!("no `{TEXT_SEPARATOR}` before the source line: {line}"))?;
    let mut words = head.split_whitespace().peekable();
    let file = words.next().ok_or("no file name")?;
    let (keyword, rule) = words
        .next()
        .and_then(|word| word.split_once('='))
        .ok_or_else(|| format!("no `kind=rule` after the file name: {line}"))?;
    if !RuleId::ALL.iter().any(|known| known.slug() == rule) {
        return Err(format!("no rule is called `{rule}`"));
    }
    let repeats = take_number(&mut words, 'x')?.unwrap_or(1);
    let occurrence = take_number(&mut words, '#')?.unwrap_or(1);
    let reason = words.collect::<Vec<_>>().join(" ");
    let verdict = verdict_from(keyword, reason)?;
    if repeats > 1 && !verdict.counts_repeats() {
        return Err(format!("`{keyword}=` takes no repeat count"));
    }
    Ok(Expectation {
        file: file.to_owned(),
        rule: rule.to_owned(),
        verdict,
        repeats,
        text: text.trim().to_owned(),
        occurrence: occurrence as usize,
    })
}

fn split_source_text(line: &str) -> Option<(&str, &str)> {
    line.split_once(TEXT_SEPARATOR)
        .or_else(|| Some((line.strip_suffix(TEXT_SEPARATOR.trim_end())?, "")))
}

fn take_number<'a>(
    words: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
    prefix: char,
) -> Result<Option<u32>, String> {
    let Some(digits) = words.peek().and_then(|word| word.strip_prefix(prefix)) else {
        return Ok(None);
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(None);
    }
    let number = digits
        .parse()
        .ok()
        .filter(|number| *number > 0)
        .ok_or_else(|| format!("`{prefix}{digits}` is not a positive number"))?;
    words.next();
    Ok(Some(number))
}

fn verdict_from(keyword: &str, reason: String) -> Result<Verdict, String> {
    match (keyword, reason.is_empty()) {
        ("known-fp", false) => Ok(Verdict::KnownFalsePositive { reason }),
        ("known-fp", true) => Err("`known-fp=` needs a reason".to_owned()),
        ("rule", true) => Ok(Verdict::Fires),
        ("none", true) => Ok(Verdict::Silent),
        ("may", true) => Ok(Verdict::Either),
        ("rule" | "none" | "may", false) => Err(format!("`{keyword}=` takes no reason: {reason}")),
        _ => Err(format!("unknown kind `{keyword}=`")),
    }
}

/// The whole file. Entries are written in the order given.
pub fn render(label: &str, expectations: &[Expectation]) -> String {
    let mut out = format!(
        "# Findings expected in {label}: file, expectation, source line.\n\
         # rule= fires on the line (xN: N times), none= must not, may= either way,\n\
         # known-fp= fires and is wrong. #N: the Nth line of the file with this text.\n\
         # Rewrite the rule= entries: {BLESS_COMMAND}\n"
    );
    for expectation in expectations {
        out.push_str(&render_entry(expectation));
        out.push('\n');
    }
    out
}

fn render_entry(expectation: &Expectation) -> String {
    let mut head = format!(
        "{} {}={}",
        expectation.file,
        expectation.verdict.keyword(),
        expectation.rule
    );
    if expectation.repeats > 1 {
        head.push_str(&format!(" x{}", expectation.repeats));
    }
    if expectation.occurrence > 1 {
        head.push_str(&format!(" #{}", expectation.occurrence));
    }
    if let Verdict::KnownFalsePositive { reason } = &expectation.verdict {
        head.push(' ');
        head.push_str(reason);
    }
    if expectation.text.is_empty() {
        format!("{head}{}", TEXT_SEPARATOR.trim_end())
    } else {
        format!("{head}{TEXT_SEPARATOR}{}", expectation.text)
    }
}
