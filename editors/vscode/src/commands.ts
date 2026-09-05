import {
  CodeAction,
  Disposable,
  Position,
  Range,
  Selection,
  Uri,
  ViewColumn,
  commands,
  env,
  window,
  workspace,
} from "vscode";
import { execFile } from "node:child_process";

import { Dashboard, opensInEditor } from "./dashboard";
import { Filters } from "./filters";
import { Finding, FindingsIndex, absolute } from "./findings-index";
import { LanguageServer } from "./language-server";
import { Panel } from "./panel";
import { FilesTree } from "./views/files-tree";
import { FindingsTree } from "./views/findings-tree";
import { RulesTree } from "./views/rules-tree";

const CONFIG_FILE = "RQX.toml";

export interface Surfaces {
  readonly server: LanguageServer;
  readonly index: FindingsIndex;
  readonly filters: Filters;
  readonly panel: Panel;
  readonly dashboard: Dashboard;
  readonly findings: FindingsTree;
  readonly files: FilesTree;
  readonly rules: RulesTree;
  readonly scanning: (message: string) => void;
  readonly settle: () => void;
  readonly showLog: () => void;
}

export function register(surfaces: Surfaces): Disposable[] {
  const { server, index, filters, panel, dashboard, files, rules } = surfaces;

  const busy = async <T>(message: string, work: () => Promise<T>): Promise<T> => {
    surfaces.scanning(message);
    try {
      return await work();
    } finally {
      surfaces.settle();
    }
  };

  const entries: [string, (...args: never[]) => unknown][] = [
    ["rqx.scanWorkspace", () => busy("scanning", () => server.send("rqx.scan"))],
    ["rqx.check", () => review(surfaces, busy)],
    ["rqx.explain", () => explainAtCursor(server)],
    ["rqx.explainRule", (slug?: string) => explainRule(server, slug)],
    ["rqx.startHere", () => startHere(server, panel)],
    ["rqx.disableRule", (target?: Finding | string) => disable(server, target)],
    ["rqx.togglePatterns", () => toggle(server, "rqx.togglePatterns", "patterns")],
    ["rqx.toggleCodeLens", () => toggle(server, "rqx.toggleCodeLens", "codeLens")],
    ["rqx.openPanel", () => panel.reveal()],
    ["rqx.restart", () => busy("reloading", () => server.restart("asked to restart"))],
    ["rqx.showOutput", () => surfaces.showLog()],

    ["rqx.openDashboard", () => dashboard.open(opensInEditor())],
    ["rqx.openDashboardExternal", () => dashboard.open(false)],

    ["rqx.openFinding", (finding: Finding) => openFinding(finding)],
    ["rqx.copyFinding", (finding: Finding) => copyFinding(finding)],
    ["rqx.filterToRule", (target: Finding | string) => filterToRule(filters, target)],

    ["rqx.fixFile", () => fixFile()],
    ["rqx.fixFinding", (finding: Finding) => fixFinding(finding)],
    ["rqx.fixWorkspace", () => fixWorkspace(surfaces, busy)],
    ["rqx.suppressFinding", (finding?: Finding) => suppress(surfaces, finding)],

    ["rqx.plan", () => plan(surfaces)],
    ["rqx.verify", () => verify(surfaces, busy)],
    ["rqx.baseline", () => baseline(surfaces, busy)],

    ["rqx.filterFindings", () => filters.pick(index)],
    ["rqx.groupFindings", () => filters.pickGrouping()],
    ["rqx.clearFilters", () => filters.clear()],
    ["rqx.filesSort", () => files.pickSort()],
    ["rqx.rulesShowFired", () => rules.toggleFiredOnly()],

    ["rqx.exportReport", () => exportReport(server)],
    ["rqx.openConfig", () => openConfig(server)],
    ["rqx.showConfig", () => showConfig(server)],
    ["rqx.setLevel", () => setLevel()],
  ];

  return entries.map(([name, run]) =>
    commands.registerCommand(name, run as (...args: unknown[]) => unknown),
  );
}

/* ── moving around ─────────────────────────────────────────────────────── */

async function openFinding(finding: Finding): Promise<void> {
  const document = await workspace.openTextDocument(finding.uri);
  const editor = await window.showTextDocument(document, { preserveFocus: false });
  editor.selection = new Selection(finding.range.start, finding.range.start);
  editor.revealRange(finding.range, 2);
}

async function startHere(server: LanguageServer, panel: Panel): Promise<void> {
  const ranked = await server.send<{ file: string }[]>("rqx.startHere");
  const first = ranked?.[0]?.file;
  const uri = first ? absolute(first) : undefined;
  if (!uri) {
    window.showInformationMessage("Rqx: nothing to open yet — scan the workspace first.");
    return;
  }
  await window.showTextDocument(await workspace.openTextDocument(uri));
  panel.reveal();
}

async function copyFinding(finding: Finding): Promise<void> {
  const technique = finding.technique.length ? `\nFix — ${finding.technique.join(" / ")}` : "";
  await env.clipboard.writeText(
    `${finding.file}:${finding.range.start.line + 1} \`${finding.rule}\`\n${finding.evidence}${technique}`,
  );
  window.setStatusBarMessage("Rqx: finding copied", 2000);
}

function filterToRule(filters: Filters, target: Finding | string): Thenable<void> {
  const rule = typeof target === "string" ? target : target.rule;
  return filters.update({ rules: [rule] }).then(() =>
    commands.executeCommand("rqx.findings.focus"),
  );
}

/* ── explaining ────────────────────────────────────────────────────────── */

/// `rqx explain` already renders the whole catalogue entry, so the editor opens that
/// markdown rather than paraphrasing it into a second, thinner description.
async function show(markdown: string, language = "markdown"): Promise<void> {
  const document = await workspace.openTextDocument({ content: markdown, language });
  await window.showTextDocument(document, { viewColumn: ViewColumn.Beside, preview: true });
  if (language === "markdown") {
    await commands.executeCommand("markdown.showPreview");
  }
}

async function explainRule(server: LanguageServer, slug?: string): Promise<void> {
  if (!slug) {
    return;
  }
  const answer = await server.send<{ markdown?: string }>("rqx.explain", slug);
  if (answer?.markdown) {
    await show(answer.markdown);
  }
}

async function explainAtCursor(server: LanguageServer): Promise<void> {
  const editor = window.activeTextEditor;
  if (!editor) {
    return;
  }
  const slug = server.ruleAt(editor.document.uri, editor.selection.active.line);
  if (!slug) {
    window.showInformationMessage("Rqx: no finding on this line.");
    return;
  }
  await explainRule(server, slug);
}

async function disable(server: LanguageServer, target?: Finding | string): Promise<void> {
  let slug = typeof target === "string" ? target : target?.rule;
  if (!slug) {
    const editor = window.activeTextEditor;
    slug = editor ? server.ruleAt(editor.document.uri, editor.selection.active.line) : undefined;
  }
  if (!slug) {
    window.showInformationMessage("Rqx: no finding on this line.");
    return;
  }
  await server.send("rqx.disableRule", slug);
}

async function toggle(server: LanguageServer, command: string, setting: string): Promise<void> {
  const answer = await server.send<Record<string, boolean>>(command);
  const on = answer?.[setting];
  if (on !== undefined) {
    // Written back so the choice survives a restart; the server holds it only for the
    // life of the process.
    await workspace.getConfiguration("rqx").update(setting, on, true);
    window.showInformationMessage(`Rqx: ${setting} ${on ? "on" : "off"}.`);
  }
}

/* ── fixing ────────────────────────────────────────────────────────────── */

/// The server answers a whole-document fix-all request with one action carrying every
/// proven edit in the file, so `apply: "first"` applies the file rather than a line.
function fixFile(): Thenable<unknown> {
  if (!window.activeTextEditor) {
    window.showInformationMessage("Rqx: open a file to fix it.");
    return Promise.resolve(undefined);
  }
  return commands.executeCommand("editor.action.codeAction", {
    kind: "source.fixAll.rqx",
    apply: "first",
  });
}

/// One finding rather than the file: the actions offered at a range are per finding,
/// and the title names the rule each one answers.
async function fixFinding(finding: Finding): Promise<void> {
  await openFinding(finding);
  const actions = await commands.executeCommand<CodeAction[]>(
    "vscode.executeCodeActionProvider",
    finding.uri,
    finding.range,
  );
  const wanted = actions?.find((action) => action.title === `rqx: fix \`${finding.rule}\``);
  if (!wanted?.edit) {
    window.showInformationMessage(
      `Rqx: \`${finding.rule}\` has no mechanical fix here — it is a decision.`,
    );
    return;
  }
  await workspace.applyEdit(wanted.edit);
}

async function fixWorkspace(
  surfaces: Surfaces,
  busy: <T>(message: string, work: () => Promise<T>) => Promise<T>,
): Promise<void> {
  type Summary = {
    summary?: { files: number; applied: number; withheld: number };
    withheld?: { file: string; reason: string }[];
    error?: string;
  };
  const preview = await busy("planning the fixes", () =>
    surfaces.server.send<Summary>("rqx.fix", false),
  );
  if (!preview || preview.error) {
    window.showWarningMessage(`Rqx: ${preview?.error ?? "the fixer did not answer"}.`);
    return;
  }
  const applied = preview.summary?.applied ?? 0;
  if (applied === 0) {
    window.showInformationMessage("Rqx: nothing here can be fixed mechanically.");
    return;
  }
  const withheld = preview.summary?.withheld ?? 0;
  const choice = await window.showInformationMessage(
    `Rqx: ${applied} proven edits across ${preview.summary?.files ?? 0} files${
      withheld > 0 ? `, ${withheld} withheld` : ""
    }. Apply them?`,
    { modal: true },
    "Apply",
  );
  if (choice !== "Apply") {
    return;
  }
  const written = await busy("applying the fixes", () =>
    surfaces.server.send<Summary>("rqx.fix", true),
  );
  window.showInformationMessage(
    `Rqx: applied ${written?.summary?.applied ?? 0} edits across ${written?.summary?.files ?? 0} files.`,
  );
}

/// A waiver is a comment above the code it excuses, and rqx refuses one with no
/// reason — so the reason is asked for rather than defaulted.
async function suppress(surfaces: Surfaces, finding?: Finding): Promise<void> {
  const target = finding ?? findingAtCursor(surfaces.index);
  if (!target) {
    window.showInformationMessage("Rqx: no finding on this line.");
    return;
  }
  const reason = await window.showInputBox({
    title: `Waive \`${target.rule}\``,
    prompt: "rqx will not honour a waiver without a reason",
    placeHolder: "the driver throws on close",
  });
  if (!reason?.trim()) {
    return;
  }
  const until = await window.showInputBox({
    title: "Revisit by",
    prompt: "YYYY-MM-DD, or leave empty for a waiver with no date",
    validateInput: (value) =>
      !value.trim() || /^\d{4}-\d{2}-\d{2}$/.test(value.trim())
        ? undefined
        : "a date reads YYYY-MM-DD",
  });
  if (until === undefined) {
    return;
  }
  const answer = await surfaces.server.send<{ error?: string }>("rqx.suppress", {
    file: target.file,
    line: target.range.start.line + 1,
    rule: target.rule,
    reason: reason.trim(),
    until: until.trim() || null,
  });
  if (answer?.error) {
    window.showWarningMessage(`Rqx: ${answer.error}`);
    return;
  }
  // The server analyses on save, so the squiggle would sit under the waiver until the
  // next one otherwise.
  const document = workspace.textDocuments.find(
    (open) => open.uri.toString() === target.uri.toString(),
  );
  await document?.save();
}

function findingAtCursor(index: FindingsIndex): Finding | undefined {
  const editor = window.activeTextEditor;
  if (!editor) {
    return undefined;
  }
  const line = editor.selection.active.line;
  return index
    .findings.filter((finding) => finding.uri.toString() === editor.document.uri.toString())
    .find((finding) => finding.range.start.line <= line && line <= finding.range.end.line);
}

/* ── the other three commands ──────────────────────────────────────────── */

async function plan(surfaces: Surfaces): Promise<void> {
  type Item = {
    rule: string;
    file: string;
    line: number;
    effort: number;
    healthAfter: number;
    technique: string[];
  };
  const budget = await window.showInputBox({
    title: "Plan a budget",
    prompt: "Effort points to spend; empty ranks everything",
    validateInput: (value) =>
      !value.trim() || /^\d+$/.test(value.trim()) ? undefined : "a number of points",
  });
  if (budget === undefined) {
    return;
  }
  const answer = await surfaces.server.send<{
    healthBefore?: number;
    healthAfter?: number;
    items?: Item[];
    error?: string;
  }>("rqx.plan", budget.trim() ? Number(budget.trim()) : null);
  if (!answer || answer.error || !answer.items?.length) {
    window.showInformationMessage(`Rqx: ${answer?.error ?? "nothing worth ranking yet"}.`);
    return;
  }
  const chosen = await window.showQuickPick(
    answer.items.map((item, index) => ({
      label: `${index + 1}. ${item.rule}`,
      description: `${item.file}:${item.line}`,
      detail: `${item.effort} points · health ${item.healthAfter} after · ${item.technique.join(" / ")}`,
      item,
    })),
    { title: `Plan — health ${answer.healthBefore} → ${answer.healthAfter}`, matchOnDetail: true },
  );
  const uri = chosen ? absolute(chosen.item.file) : undefined;
  if (chosen && uri) {
    const document = await workspace.openTextDocument(uri);
    const editor = await window.showTextDocument(document);
    const at = new Position(Math.max(chosen.item.line - 1, 0), 0);
    editor.selection = new Selection(at, at);
    editor.revealRange(new Range(at, at), 2);
  }
}

async function verify(
  surfaces: Surfaces,
  busy: <T>(message: string, work: () => Promise<T>) => Promise<T>,
): Promise<void> {
  type Change = { outcome: string; rule: string; subject: string; technique: string };
  const base = workspace.getConfiguration("rqx").get<string>("check.base", "origin/HEAD");
  const answer = await busy("verifying", () =>
    surfaces.server.send<{
      base?: string;
      passed?: boolean;
      healthBefore?: number;
      healthAfter?: number;
      unmet?: string[];
      changes?: Change[];
      error?: string;
    }>("rqx.verify", base),
  );
  if (!answer || answer.error) {
    window.showWarningMessage(`Rqx: ${answer?.error ?? "the verification did not run"}.`);
    return;
  }
  const lines = [
    `# Verify against \`${answer.base}\``,
    "",
    `**${answer.passed ? "The refactoring landed" : "The refactoring did not land"}** — health ${answer.healthBefore} → ${answer.healthAfter}.`,
    "",
    ...(answer.unmet?.length ? [`Unmet: ${answer.unmet.join(", ")}`, ""] : []),
    "| Outcome | Rule | Subject | Technique |",
    "|---|---|---|---|",
    ...(answer.changes ?? []).map(
      (change) =>
        `| ${change.outcome} | \`${change.rule}\` | ${change.subject} | ${change.technique} |`,
    ),
  ];
  await show(lines.join("\n"));
}

async function baseline(
  surfaces: Surfaces,
  busy: <T>(message: string, work: () => Promise<T>) => Promise<T>,
): Promise<void> {
  const choice = await window.showInformationMessage(
    "Rqx: record every finding the project has now, so the gate judges only what a change adds?",
    { modal: true },
    "Take the baseline",
  );
  if (choice !== "Take the baseline") {
    return;
  }
  const answer = await busy("taking the baseline", () =>
    surfaces.server.send<{ recorded?: number; path?: string; error?: string }>("rqx.baseline", true),
  );
  if (!answer || answer.error) {
    window.showWarningMessage(`Rqx: ${answer?.error ?? "the baseline was not taken"}.`);
    return;
  }
  window.showInformationMessage(`Rqx: recorded ${answer.recorded} findings in ${answer.path}.`);
}

async function review(
  surfaces: Surfaces,
  busy: <T>(message: string, work: () => Promise<T>) => Promise<T>,
): Promise<void> {
  const base = workspace.getConfiguration("rqx").get<string>("check.base", "origin/HEAD");
  const verdict = await busy("reviewing", () =>
    surfaces.server.send<{ passed: boolean; reviewedFindings: number; base: string }>(
      "rqx.check",
      base,
    ),
  );
  if (verdict?.base === undefined) {
    return;
  }
  window.showInformationMessage(
    `Rqx: ${verdict.reviewedFindings} findings against ${verdict.base} — ${
      verdict.passed ? "passes the gate" : "fails the gate"
    }.`,
  );
}

/* ── configuration ─────────────────────────────────────────────────────── */

async function showConfig(server: LanguageServer): Promise<void> {
  type Row = { table: string; name: string; value: string; isDefault: boolean; source: string };
  const answer = await server.send<{ level?: string; sources?: string[]; settings?: Row[] }>(
    "rqx.config",
  );
  if (!answer?.settings) {
    return;
  }
  const lines = [
    "# The settings this project is being read with",
    "",
    `Level **${answer.level}** · ${answer.sources?.length ? answer.sources.join(", ") : "no config file; every value is a default"}`,
    "",
    "| Setting | Value | From |",
    "|---|---|---|",
    ...answer.settings.map((row) => `| ${row.table}.${row.name} | ${row.value} | ${row.source} |`),
  ];
  await show(lines.join("\n"));
}

async function openConfig(server: LanguageServer): Promise<void> {
  const uri = absolute(CONFIG_FILE);
  if (!uri) {
    return;
  }
  const exists = await workspace.fs.stat(uri).then(
    () => true,
    () => false,
  );
  if (!exists) {
    const choice = await window.showInformationMessage(
      "Rqx: this project has no RQX.toml. Write one with the defaults spelled out?",
      "Write it",
    );
    if (choice !== "Write it") {
      return;
    }
    if (!(await runBinary(server, ["init"]))) {
      return;
    }
  }
  await window.showTextDocument(await workspace.openTextDocument(uri));
}

/// The level is one line in `RQX.toml`, so it is edited rather than re-serialised:
/// `rqx init` writes a line of prose per table and a TOML round trip deletes all of it.
async function setLevel(): Promise<void> {
  const levels = ["relaxed", "standard", "strict", "pristine"];
  const chosen = await window.showQuickPick(levels, { title: "Hold this project to" });
  const uri = chosen ? absolute(CONFIG_FILE) : undefined;
  if (!chosen || !uri) {
    return;
  }
  const existing = await workspace.fs.readFile(uri).then(
    (bytes) => Buffer.from(bytes).toString("utf8"),
    () => "",
  );
  const line = `level = "${chosen}"`;
  const written = /^\s*level\s*=.*$/m.test(existing)
    ? existing.replace(/^\s*level\s*=.*$/m, line)
    : `${line}\n${existing}`;
  await workspace.fs.writeFile(uri, Buffer.from(written, "utf8"));
  await window.showTextDocument(await workspace.openTextDocument(uri));
}

/* ── exporting ─────────────────────────────────────────────────────────── */

const REPORTERS: Record<string, string> = {
  json: "json",
  sarif: "sarif",
  markdown: "markdown",
  junit: "junit",
};

async function exportReport(server: LanguageServer): Promise<void> {
  const format = await window.showQuickPick(Object.keys(REPORTERS), {
    title: "Write the report as",
  });
  if (!format) {
    return;
  }
  const root = workspace.workspaceFolders?.[0]?.uri;
  const target = await window.showSaveDialog({
    defaultUri: root ? Uri.joinPath(root, `rqx.${format === "markdown" ? "md" : format}`) : undefined,
    title: "Write the report to",
  });
  if (!target) {
    return;
  }
  const wrote = await runBinary(server, [
    "scan",
    "--reporter",
    `${REPORTERS[format]}=${target.fsPath}`,
  ]);
  if (wrote) {
    window.showInformationMessage(`Rqx: wrote ${target.fsPath}.`);
  }
}

/// The reporters write files rather than answer a request, so this is the one place
/// the extension runs the binary instead of asking the server.
function runBinary(server: LanguageServer, args: string[]): Promise<boolean> {
  const binary = server.executable;
  const cwd = workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (!binary || !cwd) {
    window.showWarningMessage("Rqx: no binary to run — see the log.");
    return Promise.resolve(false);
  }
  return new Promise((resolve) => {
    execFile(binary, args, { cwd, timeout: 300_000 }, (error, _out, stderr) => {
      // `rqx scan` exits non-zero when the gate fails, which is not a failure to write
      // the report the caller asked for.
      if (error && typeof error.code === "number" && error.code > 1) {
        window.showWarningMessage(`Rqx: ${stderr.trim() || error.message}`);
        resolve(false);
        return;
      }
      resolve(true);
    });
  });
}
