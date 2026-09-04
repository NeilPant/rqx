use std::fmt::Write as _;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars::{self, JsonSchema};
use rmcp::{ErrorData as McpError, tool, tool_router};
use rqx_core::finding::{Finding, Severity};
use rqx_core::rule::{Category, RuleId};
use rqx_core::{Advice, Language, Stance};
use serde::Deserialize;
use serde_json::{Value, json};

use super::Rqx;
use super::root::{Cursor, within};
use crate::brand::{ColorDepth, Palette};
use crate::report::{
    self, FindingFilter, Layout, Report, Reporter, finding_of, projection, setting_rows,
};
use crate::{check, scan};

const PAGE: usize = 20;
const MAX_PAGE: usize = 100;
/// Enough to say where to start without listing a project's whole file tree.
const RANKED: usize = 10;

#[derive(Deserialize, JsonSchema, Default)]
pub struct Where {
    /// Directory or file to scan, relative to the server's root. Defaults to the root.
    pub path: Option<String>,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct Page {
    /// Lowest severity to return: `info`, `warning` or `error`.
    pub severity: Option<String>,
    /// Rule slugs to keep, e.g. `long-function`. Empty keeps every rule.
    #[serde(default)]
    pub rule: Vec<String>,
    /// Rule families to keep, e.g. `waste`. Empty keeps every family.
    #[serde(default)]
    pub category: Vec<String>,
    /// Path globs to keep, e.g. `src/orders/**`. Empty keeps every file.
    #[serde(default)]
    pub glob: Vec<String>,
    /// How many findings to return. Defaults to 20, capped at 100.
    pub limit: Option<u32>,
    /// The `next_cursor` from a previous answer.
    pub cursor: Option<String>,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct Diff {
    /// Revision to compare against, e.g. `main` or `HEAD~1`.
    pub base: String,
    /// Directory to review, relative to the server's root. Defaults to the root.
    pub path: Option<String>,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct Buffer {
    /// The source to analyse. Never written to disk.
    pub source: String,
    /// A name for the buffer, used to pick the language and to label the findings.
    pub filename: Option<String>,
    /// Language slug, when the filename does not say: `typescript`, `python`, `go`,
    /// `java` or `cpp`.
    pub language: Option<String>,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct Named {
    /// A rule slug (`unused-export`), a pattern (`decorator`) or an advisory.
    pub name: String,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct Catalogue {
    /// Substring to match against a rule's slug, metric or reason.
    pub query: Option<String>,
    /// Family slug to keep, e.g. `size`.
    pub category: Option<String>,
    /// Keep only rules that apply to this language.
    pub language: Option<String>,
    /// Include the rules that ship off by default. Off by default here too.
    pub include_off: Option<bool>,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct Advisories {
    /// `recognized`, `incomplete` or `applicable`. Empty returns all three.
    pub stance: Option<String>,
    /// How many advisories to return. Defaults to 20, capped at 100.
    pub limit: Option<u32>,
    /// The `next_cursor` from a previous answer.
    pub cursor: Option<String>,
}

#[tool_router(vis = "pub(super)")]
impl Rqx {
    #[tool(
        name = "scan",
        description = "Analyse the project and answer with its health, its grade \
                       distribution and the files to open first. Returns no findings — \
                       page those out of `findings`, which reads this same scan.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn scan(&self, Parameters(asked): Parameters<Where>) -> Result<CallToolResult, McpError> {
        let scanned = within(&self.root, asked.path.as_deref()).map_err(refused)?;
        let snapshot = self.snapshot(Some(scanned)).await?;
        let outcome = &snapshot.outcome;
        let report = Report::everything(outcome);
        let priority = report.priority();
        let ranked_below = priority.len().saturating_sub(RANKED);
        let start_here = ranked(&priority, outcome);

        let structured = json!({
            "scan_id": snapshot.id,
            "scanned": self.relative(&snapshot.scanned),
            "health": report.health(),
            "files_scanned": outcome.files_scanned,
            "code_lines": outcome.code_lines,
            "elapsed_ms": outcome.elapsed_ms,
            "findings": {
                "total": outcome.counts.total,
                "by_severity": outcome.counts.by_severity,
                "by_rule": outcome.counts.by_rule,
            },
            "grades": report
                .grades()
                .into_iter()
                .map(|(grade, count)| (grade.label().to_owned(), count))
                .collect::<std::collections::BTreeMap<_, _>>(),
            "start_here": start_here,
            "ranked_below": ranked_below,
            "advice": outcome.advice.len(),
            "skipped": outcome
                .skipped
                .iter()
                .map(|(path, reason)| format!("{}: {reason}", path.display()))
                .collect::<Vec<_>>(),
            "next": "`findings` pages the findings themselves; `explain` says what a rule measures.",
        });
        Ok(answer(markdown(&report)?, structured))
    }

    #[tool(
        name = "findings",
        description = "Page the findings of the most recent scan, narrowed by severity, \
                       rule, family or path glob. Scans the project first if nothing has \
                       been scanned yet.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn findings(
        &self,
        Parameters(page): Parameters<Page>,
    ) -> Result<CallToolResult, McpError> {
        let snapshot = self.snapshot(None).await?;
        let filter = FindingFilter::new(
            severity_of(page.severity.as_deref())?,
            &page.rule,
            &page.category,
            &page.glob,
        )
        .map_err(|error| refused(format!("{error:#}")))?;
        let report = Report::scan(&snapshot.outcome, &filter);

        let offset = match page.cursor.as_deref() {
            Some(cursor) => Cursor::parse(cursor, snapshot.id).map_err(refused)?.offset,
            None => 0,
        };
        let limit = page
            .limit
            .map_or(PAGE, |limit| (limit as usize).clamp(1, MAX_PAGE));
        let matched = report.findings.len();
        let shown: Vec<&Finding> = report
            .findings
            .iter()
            .skip(offset)
            .take(limit)
            .copied()
            .collect();
        let next = offset + shown.len();

        let structured = json!({
            "scan_id": snapshot.id,
            "matched": matched,
            "returned": shown.len(),
            "next_cursor": (next < matched).then(|| Cursor::format(snapshot.id, next)),
            "findings": shown.iter().copied().map(finding_of).collect::<Vec<_>>(),
        });
        Ok(answer(listing(&shown, offset, matched), structured))
    }

    #[tool(
        name = "check",
        description = "Review only what changed against a base revision, plus everything \
                       that imports it, and say whether the configured gate passes.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn check(&self, Parameters(diff): Parameters<Diff>) -> Result<CallToolResult, McpError> {
        let root = within(&self.root, diff.path.as_deref()).map_err(refused)?;
        let settings = std::sync::Arc::clone(&self.settings);
        let base = diff.base.clone();
        let review = tokio::task::spawn_blocking(move || check::review(&root, &settings, &base))
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?
            .map_err(|error| refused(format!("{error:#}")))?;

        let verdict = check::judge(&review, &self.settings.config);
        let report = Report::reviewed(&review, &verdict);
        let reviewed: Vec<&Finding> = review.findings().collect();

        let structured = json!({
            "base": review.base,
            "health": report.health(),
            "reviewed_files": review.reviewed.len(),
            "reviewed_findings": reviewed.len(),
            "gate": {
                "passed": verdict.passed(),
                "counted": verdict.counted,
                "failures": verdict.failures,
            },
            "findings": reviewed
                .iter()
                .take(MAX_PAGE)
                .copied()
                .map(finding_of)
                .collect::<Vec<_>>(),
            "omitted": reviewed.len().saturating_sub(MAX_PAGE),
        });
        Ok(answer(markdown(&report)?, structured))
    }

    #[tool(
        name = "analyze",
        description = "Analyse source held in memory — an unsaved buffer or code you just \
                       wrote — without touching the disk.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn analyze(
        &self,
        Parameters(buffer): Parameters<Buffer>,
    ) -> Result<CallToolResult, McpError> {
        let display = buffer.filename.as_deref().unwrap_or("<buffer>");
        let language = match buffer.language.as_deref() {
            Some(slug) => Language::parse(slug).ok_or_else(|| {
                refused(format!(
                    "`{slug}` is not a language rqx reads. It reads typescript, python, go, java \
                     and cpp."
                ))
            })?,
            None => Language::of_path(std::path::Path::new(display)).ok_or_else(|| {
                refused(format!(
                    "nothing about `{display}` says which language it is. Pass `language`."
                ))
            })?,
        };

        let config = self.settings.for_file(std::path::Path::new(display));
        let analysis = rqx_lang::analyze_as(language, display, &buffer.source, config)
            .map_err(|error| refused(error.to_string()))?;

        let structured = json!({
            "language": language.slug(),
            "code_lines": analysis.code_lines,
            "findings": analysis.findings.iter().map(finding_of).collect::<Vec<_>>(),
            "advice": analysis.advice.iter().map(projection::advisory).collect::<Vec<_>>(),
            "scope": "buffer",
            "note": buffer_scope(),
        });
        let shown: Vec<&Finding> = analysis.findings.iter().collect();
        let mut digest = listing(&shown, 0, shown.len());
        let _ = write!(digest, "\n{}\n", buffer_scope());
        Ok(answer(digest, structured))
    }

    #[tool(
        name = "explain",
        description = "What one rule measures, why it matters and which named refactoring \
                       fixes it. Also answers for a design pattern.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn explain(
        &self,
        Parameters(named): Parameters<Named>,
    ) -> Result<CallToolResult, McpError> {
        let name = named.name.trim();
        if let Some(rule) = projection::rule(name) {
            let digest = rule["markdown"].as_str().unwrap_or_default().to_owned();
            return Ok(answer(digest, rule));
        }
        let pattern = projection::pattern(name).ok_or_else(|| {
            refused(format!(
                "there is no rule or pattern called `{name}`. `rules` lists the catalogue."
            ))
        })?;
        let digest = pattern["markdown"].as_str().unwrap_or_default().to_owned();
        Ok(answer(digest, pattern))
    }

    #[tool(
        name = "rules",
        description = "Search the rule catalogue: what each rule measures, the smell it \
                       comes from and the refactoring it names.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn rules(
        &self,
        Parameters(ask): Parameters<Catalogue>,
    ) -> Result<CallToolResult, McpError> {
        let category = match ask.category.as_deref() {
            Some(slug) => Some(Category::parse(slug).ok_or_else(|| {
                refused(format!(
                    "`{slug}` is not a family. They are: {}.",
                    Category::ALL.map(Category::slug).join(", ")
                ))
            })?),
            None => None,
        };
        let language = match ask.language.as_deref() {
            Some(slug) => Some(
                Language::parse(slug)
                    .ok_or_else(|| refused(format!("`{slug}` is not a language rqx reads.")))?,
            ),
            None => None,
        };
        let query = ask.query.unwrap_or_default().to_lowercase();

        let matched: Vec<Value> = RuleId::ALL
            .iter()
            .filter(|rule| category.is_none_or(|family| rule.spec().category == family))
            .filter(|rule| language.is_none_or(|language| rule.exemption(language).is_none()))
            .filter(|rule| ask.include_off.unwrap_or(false) || !rule.spec().off_by_default)
            .filter(|rule| {
                let spec = rule.spec();
                query.is_empty()
                    || [spec.slug, spec.metric, spec.why]
                        .iter()
                        .any(|field| field.to_lowercase().contains(&query))
            })
            .map(|rule| projection::rule(rule.spec().slug).unwrap_or(Value::Null))
            .collect();

        let digest = matched
            .iter()
            .map(|rule| {
                format!(
                    "`{}` — {}",
                    rule["slug"].as_str().unwrap_or_default(),
                    rule["metric"].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(answer(
            digest,
            json!({ "matched": matched.len(), "rules": matched }),
        ))
    }

    #[tool(
        name = "patterns",
        description = "Design patterns the project uses, half applies or has a shape for. \
                       None of it moves the health score.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn patterns(
        &self,
        Parameters(ask): Parameters<Advisories>,
    ) -> Result<CallToolResult, McpError> {
        let snapshot = self.snapshot(None).await?;
        let stance = match ask.stance.as_deref() {
            Some(slug) => Some(Stance::parse(slug).ok_or_else(|| {
                refused(format!(
                    "`{slug}` is not a stance. They are: recognized, incomplete, applicable."
                ))
            })?),
            None => None,
        };
        let matched: Vec<&Advice> = snapshot
            .outcome
            .advice
            .iter()
            .filter(|advice| stance.is_none_or(|stance| advice.advisory.spec().stance == stance))
            .collect();

        let offset = match ask.cursor.as_deref() {
            Some(cursor) => Cursor::parse(cursor, snapshot.id).map_err(refused)?.offset,
            None => 0,
        };
        let limit = ask
            .limit
            .map_or(PAGE, |limit| (limit as usize).clamp(1, MAX_PAGE));
        let shown: Vec<&Advice> = matched.iter().skip(offset).take(limit).copied().collect();
        let next = offset + shown.len();

        let digest = shown
            .iter()
            .map(|advice| {
                let spec = advice.advisory.spec();
                format!(
                    "`{}:{}` {} — {} ({})",
                    advice.location.file,
                    advice.location.line,
                    spec.pattern.name(),
                    advice.note,
                    spec.stance.label()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(answer(
            digest,
            json!({
                "scan_id": snapshot.id,
                "matched": matched.len(),
                "returned": shown.len(),
                "next_cursor": (next < matched.len()).then(|| Cursor::format(snapshot.id, next)),
                    "advice": shown.iter().copied().map(projection::advisory).collect::<Vec<_>>(),
            }),
        ))
    }

    #[tool(
        name = "config",
        description = "The settings this server will actually use, and which file each \
                       value came from.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn config(&self) -> Result<CallToolResult, McpError> {
        let rows = setting_rows(&self.settings);
        let digest = rows
            .iter()
            .map(|row| {
                format!(
                    "{}.{} = {} ({})",
                    row.table,
                    row.name,
                    row.value,
                    row.source(&self.settings)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        // File names rather than paths, the way `Row::source` already names provenance:
        // an agent has no use for the server's directory layout, and over HTTP it is not
        // the caller's to know.
        let structured = json!({
            "sources": self
                .settings
                .sources
                .iter()
                .filter_map(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
                .collect::<Vec<_>>(),
            "settings": rows
                .iter()
                .map(|row| json!({
                    "table": row.table,
                    "name": row.name,
                    "value": row.value,
                    "isDefault": row.is_default,
                    "source": row.source(&self.settings),
                }))
                .collect::<Vec<_>>(),
        });
        Ok(answer(digest, structured))
    }
}

/// An agent that gets no `unused-export` back from one buffer will otherwise read the
/// silence as a clean bill of health.
fn buffer_scope() -> String {
    format!(
        "One buffer, so {} of the {} rules ran. Unused exports, unused files, import \
         cycles, duplication and the two co-change rules need the whole project — call \
         `scan` for those.",
        RuleId::ALL.len() - RuleId::PROJECT_SCOPED.len(),
        RuleId::ALL.len()
    )
}

fn answer(digest: String, structured: Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(digest)]);
    result.structured_content = Some(structured);
    result
}

fn refused(why: impl Into<String>) -> McpError {
    McpError::invalid_params(why.into(), None)
}

fn severity_of(named: Option<&str>) -> Result<Option<Severity>, McpError> {
    match named.map(str::trim) {
        None | Some("") => Ok(None),
        Some("info") => Ok(Some(Severity::Info)),
        Some("warning") => Ok(Some(Severity::Warning)),
        Some("error") => Ok(Some(Severity::Error)),
        Some(other) => Err(refused(format!(
            "`{other}` is not a severity. They are: info, warning, error."
        ))),
    }
}

fn ranked(priority: &[(&str, rqx_core::Cleanliness)], outcome: &scan::ScanOutcome) -> Vec<Value> {
    priority
        .iter()
        .take(RANKED)
        .map(|(file, grade)| {
            let impact = outcome.impact.get(file);
            json!({
                "file": file,
                "grade": grade.label(),
                "findings": outcome.findings_by_file.get(*file).copied().unwrap_or(0),
                "importers": impact.importers,
                "commits": impact.commits,
            })
        })
        .collect()
}

/// The Markdown reporter already writes a bounded digest — health, the severity and
/// rule tables, ten findings — so a second summariser here would be one more thing to
/// keep in step with the reporters.
fn markdown(report: &Report<'_>) -> Result<String, McpError> {
    let layout = Layout {
        palette: Palette::with_depth(ColorDepth::Monochrome),
        show_passed: false,
        quiet: false,
        context: 0,
    };
    let mut rendered = Vec::new();
    report::emit(report, Reporter::Markdown, &layout, &mut rendered)
        .map_err(|error| McpError::internal_error(format!("{error:#}"), None))?;
    String::from_utf8(rendered).map_err(|error| McpError::internal_error(error.to_string(), None))
}

fn listing(findings: &[&Finding], offset: usize, matched: usize) -> String {
    let mut text = String::new();
    for finding in findings {
        let spec = finding.rule.spec();
        let _ = writeln!(
            text,
            "`{}:{}:{}` **{}** {} — {} Fix: {}",
            finding.location.file,
            finding.location.line,
            finding.location.column,
            spec.slug,
            finding.severity.label(),
            finding.evidence,
            spec.fixes
                .iter()
                .map(|fix| fix.name())
                .collect::<Vec<_>>()
                .join(" / "),
        );
    }
    let shown = offset + findings.len();
    let _ = write!(text, "\nShowing {}-{shown} of {matched}.", offset + 1);
    text
}
