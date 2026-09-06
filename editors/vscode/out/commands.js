"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.register = register;
const vscode_1 = require("vscode");
const node_child_process_1 = require("node:child_process");
const dashboard_1 = require("./dashboard");
const findings_index_1 = require("./findings-index");
const CONFIG_FILE = "RQX.toml";
function register(surfaces) {
    const { server, index, filters, panel, dashboard, files, rules } = surfaces;
    const busy = async (message, work) => {
        surfaces.scanning(message);
        try {
            return await work();
        }
        finally {
            surfaces.settle();
        }
    };
    const entries = [
        ["rqx.scanWorkspace", () => busy("scanning", () => server.send("rqx.scan"))],
        ["rqx.check", () => review(surfaces, busy)],
        ["rqx.explain", () => explainAtCursor(server)],
        ["rqx.explainRule", (slug) => explainRule(server, slug)],
        ["rqx.startHere", () => startHere(server, panel)],
        ["rqx.disableRule", (target) => disable(server, target)],
        ["rqx.togglePatterns", () => toggle(server, "rqx.togglePatterns", "patterns")],
        ["rqx.toggleCodeLens", () => toggle(server, "rqx.toggleCodeLens", "codeLens")],
        ["rqx.openPanel", () => panel.reveal()],
        ["rqx.restart", () => busy("reloading", () => server.restart("asked to restart"))],
        ["rqx.showOutput", () => surfaces.showLog()],
        ["rqx.openDashboard", () => dashboard.open((0, dashboard_1.opensInEditor)())],
        ["rqx.openDashboardExternal", () => dashboard.open(false)],
        ["rqx.openFinding", (finding) => openFinding(finding)],
        ["rqx.copyFinding", (finding) => copyFinding(finding)],
        ["rqx.filterToRule", (target) => filterToRule(filters, target)],
        ["rqx.fixFile", () => fixFile()],
        ["rqx.fixFinding", (finding) => fixFinding(finding)],
        ["rqx.fixWorkspace", () => fixWorkspace(surfaces, busy)],
        ["rqx.suppressFinding", (finding) => suppress(surfaces, finding)],
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
    return entries.map(([name, run]) => vscode_1.commands.registerCommand(name, run));
}
async function openFinding(finding) {
    const document = await vscode_1.workspace.openTextDocument(finding.uri);
    const editor = await vscode_1.window.showTextDocument(document, { preserveFocus: false });
    editor.selection = new vscode_1.Selection(finding.range.start, finding.range.start);
    editor.revealRange(finding.range, 2);
}
async function startHere(server, panel) {
    const ranked = await server.send("rqx.startHere");
    const first = ranked?.[0]?.file;
    const uri = first ? (0, findings_index_1.absolute)(first) : undefined;
    if (!uri) {
        vscode_1.window.showInformationMessage("Rqx: nothing to open yet — scan the workspace first.");
        return;
    }
    await vscode_1.window.showTextDocument(await vscode_1.workspace.openTextDocument(uri));
    panel.reveal();
}
async function copyFinding(finding) {
    const technique = finding.technique.length ? `\nFix — ${finding.technique.join(" / ")}` : "";
    await vscode_1.env.clipboard.writeText(`${finding.file}:${finding.range.start.line + 1} \`${finding.rule}\`\n${finding.evidence}${technique}`);
    vscode_1.window.setStatusBarMessage("Rqx: finding copied", 2000);
}
function filterToRule(filters, target) {
    const rule = typeof target === "string" ? target : target.rule;
    return filters.update({ rules: [rule] }).then(() => vscode_1.commands.executeCommand("rqx.findings.focus"));
}
/// `rqx explain` already renders the whole catalogue entry, so the editor opens that
/// markdown rather than paraphrasing it into a second, thinner description.
async function show(markdown, language = "markdown") {
    const document = await vscode_1.workspace.openTextDocument({ content: markdown, language });
    await vscode_1.window.showTextDocument(document, { viewColumn: vscode_1.ViewColumn.Beside, preview: true });
    if (language === "markdown") {
        await vscode_1.commands.executeCommand("markdown.showPreview");
    }
}
async function explainRule(server, slug) {
    if (!slug) {
        return;
    }
    const answer = await server.send("rqx.explain", slug);
    if (answer?.markdown) {
        await show(answer.markdown);
    }
}
async function explainAtCursor(server) {
    const editor = vscode_1.window.activeTextEditor;
    if (!editor) {
        return;
    }
    const slug = server.ruleAt(editor.document.uri, editor.selection.active.line);
    if (!slug) {
        vscode_1.window.showInformationMessage("Rqx: no finding on this line.");
        return;
    }
    await explainRule(server, slug);
}
async function disable(server, target) {
    let slug = typeof target === "string" ? target : target?.rule;
    if (!slug) {
        const editor = vscode_1.window.activeTextEditor;
        slug = editor ? server.ruleAt(editor.document.uri, editor.selection.active.line) : undefined;
    }
    if (!slug) {
        vscode_1.window.showInformationMessage("Rqx: no finding on this line.");
        return;
    }
    await server.send("rqx.disableRule", slug);
}
async function toggle(server, command, setting) {
    const answer = await server.send(command);
    const on = answer?.[setting];
    if (on !== undefined) {
        // Written back so the choice survives a restart; the server holds it only for the
        // life of the process.
        await vscode_1.workspace.getConfiguration("rqx").update(setting, on, true);
        vscode_1.window.showInformationMessage(`Rqx: ${setting} ${on ? "on" : "off"}.`);
    }
}
/// The server answers a whole-document fix-all request with one action carrying every
/// proven edit in the file, so `apply: "first"` applies the file rather than a line.
function fixFile() {
    if (!vscode_1.window.activeTextEditor) {
        vscode_1.window.showInformationMessage("Rqx: open a file to fix it.");
        return Promise.resolve(undefined);
    }
    return vscode_1.commands.executeCommand("editor.action.codeAction", {
        kind: "source.fixAll.rqx",
        apply: "first",
    });
}
/// One finding rather than the file: the actions offered at a range are per finding,
/// and the title names the rule each one answers.
async function fixFinding(finding) {
    await openFinding(finding);
    const actions = await vscode_1.commands.executeCommand("vscode.executeCodeActionProvider", finding.uri, finding.range);
    const wanted = actions?.find((action) => action.title === `rqx: fix \`${finding.rule}\``);
    if (!wanted?.edit) {
        vscode_1.window.showInformationMessage(`Rqx: \`${finding.rule}\` has no mechanical fix here — it is a decision.`);
        return;
    }
    await vscode_1.workspace.applyEdit(wanted.edit);
}
async function fixWorkspace(surfaces, busy) {
    const preview = await busy("planning the fixes", () => surfaces.server.send("rqx.fix", false));
    if (!preview || preview.error) {
        vscode_1.window.showWarningMessage(`Rqx: ${preview?.error ?? "the fixer did not answer"}.`);
        return;
    }
    const applied = preview.summary?.applied ?? 0;
    if (applied === 0) {
        vscode_1.window.showInformationMessage("Rqx: nothing here can be fixed mechanically.");
        return;
    }
    const withheld = preview.summary?.withheld ?? 0;
    const choice = await vscode_1.window.showInformationMessage(`Rqx: ${applied} proven edits across ${preview.summary?.files ?? 0} files${withheld > 0 ? `, ${withheld} withheld` : ""}. Apply them?`, { modal: true }, "Apply");
    if (choice !== "Apply") {
        return;
    }
    const written = await busy("applying the fixes", () => surfaces.server.send("rqx.fix", true));
    vscode_1.window.showInformationMessage(`Rqx: applied ${written?.summary?.applied ?? 0} edits across ${written?.summary?.files ?? 0} files.`);
}
/// A waiver is a comment above the code it excuses, and rqx refuses one with no
/// reason — so the reason is asked for rather than defaulted.
async function suppress(surfaces, finding) {
    const target = finding ?? findingAtCursor(surfaces.index);
    if (!target) {
        vscode_1.window.showInformationMessage("Rqx: no finding on this line.");
        return;
    }
    const reason = await vscode_1.window.showInputBox({
        title: `Waive \`${target.rule}\``,
        prompt: "rqx will not honour a waiver without a reason",
        placeHolder: "the driver throws on close",
    });
    if (!reason?.trim()) {
        return;
    }
    const until = await vscode_1.window.showInputBox({
        title: "Revisit by",
        prompt: "YYYY-MM-DD, or leave empty for a waiver with no date",
        validateInput: (value) => !value.trim() || /^\d{4}-\d{2}-\d{2}$/.test(value.trim())
            ? undefined
            : "a date reads YYYY-MM-DD",
    });
    if (until === undefined) {
        return;
    }
    const answer = await surfaces.server.send("rqx.suppress", {
        file: target.file,
        line: target.range.start.line + 1,
        rule: target.rule,
        reason: reason.trim(),
        until: until.trim() || null,
    });
    if (answer?.error) {
        vscode_1.window.showWarningMessage(`Rqx: ${answer.error}`);
        return;
    }
    // The server analyses on save, so the squiggle would sit under the waiver until the
    // next one otherwise.
    const document = vscode_1.workspace.textDocuments.find((open) => open.uri.toString() === target.uri.toString());
    await document?.save();
}
function findingAtCursor(index) {
    const editor = vscode_1.window.activeTextEditor;
    if (!editor) {
        return undefined;
    }
    const line = editor.selection.active.line;
    return index
        .findings.filter((finding) => finding.uri.toString() === editor.document.uri.toString())
        .find((finding) => finding.range.start.line <= line && line <= finding.range.end.line);
}
async function plan(surfaces) {
    const budget = await vscode_1.window.showInputBox({
        title: "Plan a budget",
        prompt: "Effort points to spend; empty ranks everything",
        validateInput: (value) => !value.trim() || /^\d+$/.test(value.trim()) ? undefined : "a number of points",
    });
    if (budget === undefined) {
        return;
    }
    const answer = await surfaces.server.send("rqx.plan", budget.trim() ? Number(budget.trim()) : null);
    if (!answer || answer.error || !answer.items?.length) {
        vscode_1.window.showInformationMessage(`Rqx: ${answer?.error ?? "nothing worth ranking yet"}.`);
        return;
    }
    const chosen = await vscode_1.window.showQuickPick(answer.items.map((item, index) => ({
        label: `${index + 1}. ${item.rule}`,
        description: `${item.file}:${item.line}`,
        detail: `${item.effort} points · health ${item.healthAfter} after · ${item.technique.join(" / ")}`,
        item,
    })), { title: `Plan — health ${answer.healthBefore} → ${answer.healthAfter}`, matchOnDetail: true });
    const uri = chosen ? (0, findings_index_1.absolute)(chosen.item.file) : undefined;
    if (chosen && uri) {
        const document = await vscode_1.workspace.openTextDocument(uri);
        const editor = await vscode_1.window.showTextDocument(document);
        const at = new vscode_1.Position(Math.max(chosen.item.line - 1, 0), 0);
        editor.selection = new vscode_1.Selection(at, at);
        editor.revealRange(new vscode_1.Range(at, at), 2);
    }
}
async function verify(surfaces, busy) {
    const base = vscode_1.workspace.getConfiguration("rqx").get("check.base", "origin/HEAD");
    const answer = await busy("verifying", () => surfaces.server.send("rqx.verify", base));
    if (!answer || answer.error) {
        vscode_1.window.showWarningMessage(`Rqx: ${answer?.error ?? "the verification did not run"}.`);
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
        ...(answer.changes ?? []).map((change) => `| ${change.outcome} | \`${change.rule}\` | ${change.subject} | ${change.technique} |`),
    ];
    await show(lines.join("\n"));
}
async function baseline(surfaces, busy) {
    const choice = await vscode_1.window.showInformationMessage("Rqx: record every finding the project has now, so the gate judges only what a change adds?", { modal: true }, "Take the baseline");
    if (choice !== "Take the baseline") {
        return;
    }
    const answer = await busy("taking the baseline", () => surfaces.server.send("rqx.baseline", true));
    if (!answer || answer.error) {
        vscode_1.window.showWarningMessage(`Rqx: ${answer?.error ?? "the baseline was not taken"}.`);
        return;
    }
    vscode_1.window.showInformationMessage(`Rqx: recorded ${answer.recorded} findings in ${answer.path}.`);
}
async function review(surfaces, busy) {
    const base = vscode_1.workspace.getConfiguration("rqx").get("check.base", "origin/HEAD");
    const verdict = await busy("reviewing", () => surfaces.server.send("rqx.check", base));
    if (verdict?.base === undefined) {
        return;
    }
    vscode_1.window.showInformationMessage(`Rqx: ${verdict.reviewedFindings} findings against ${verdict.base} — ${verdict.passed ? "passes the gate" : "fails the gate"}.`);
}
async function showConfig(server) {
    const answer = await server.send("rqx.config");
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
async function openConfig(server) {
    const uri = (0, findings_index_1.absolute)(CONFIG_FILE);
    if (!uri) {
        return;
    }
    const exists = await vscode_1.workspace.fs.stat(uri).then(() => true, () => false);
    if (!exists) {
        const choice = await vscode_1.window.showInformationMessage("Rqx: this project has no RQX.toml. Write one with the defaults spelled out?", "Write it");
        if (choice !== "Write it") {
            return;
        }
        if (!(await runBinary(server, ["init"]))) {
            return;
        }
    }
    await vscode_1.window.showTextDocument(await vscode_1.workspace.openTextDocument(uri));
}
/// The level is one line in `RQX.toml`, so it is edited rather than re-serialised:
/// `rqx init` writes a line of prose per table and a TOML round trip deletes all of it.
async function setLevel() {
    const levels = ["relaxed", "standard", "strict", "pristine"];
    const chosen = await vscode_1.window.showQuickPick(levels, { title: "Hold this project to" });
    const uri = chosen ? (0, findings_index_1.absolute)(CONFIG_FILE) : undefined;
    if (!chosen || !uri) {
        return;
    }
    const existing = await vscode_1.workspace.fs.readFile(uri).then((bytes) => Buffer.from(bytes).toString("utf8"), () => "");
    const line = `level = "${chosen}"`;
    const written = /^\s*level\s*=.*$/m.test(existing)
        ? existing.replace(/^\s*level\s*=.*$/m, line)
        : `${line}\n${existing}`;
    await vscode_1.workspace.fs.writeFile(uri, Buffer.from(written, "utf8"));
    await vscode_1.window.showTextDocument(await vscode_1.workspace.openTextDocument(uri));
}
const REPORTERS = {
    json: "json",
    sarif: "sarif",
    markdown: "markdown",
    junit: "junit",
};
async function exportReport(server) {
    const format = await vscode_1.window.showQuickPick(Object.keys(REPORTERS), {
        title: "Write the report as",
    });
    if (!format) {
        return;
    }
    const root = vscode_1.workspace.workspaceFolders?.[0]?.uri;
    const target = await vscode_1.window.showSaveDialog({
        defaultUri: root ? vscode_1.Uri.joinPath(root, `rqx.${format === "markdown" ? "md" : format}`) : undefined,
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
        vscode_1.window.showInformationMessage(`Rqx: wrote ${target.fsPath}.`);
    }
}
/// The reporters write files rather than answer a request, so this is the one place
/// the extension runs the binary instead of asking the server.
function runBinary(server, args) {
    const binary = server.executable;
    const cwd = vscode_1.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!binary || !cwd) {
        vscode_1.window.showWarningMessage("Rqx: no binary to run — see the log.");
        return Promise.resolve(false);
    }
    return new Promise((resolve) => {
        (0, node_child_process_1.execFile)(binary, args, { cwd, timeout: 300_000 }, (error, _out, stderr) => {
            // `rqx scan` exits non-zero when the gate fails, which is not a failure to write
            // the report the caller asked for.
            if (error && typeof error.code === "number" && error.code > 1) {
                vscode_1.window.showWarningMessage(`Rqx: ${stderr.trim() || error.message}`);
                resolve(false);
                return;
            }
            resolve(true);
        });
    });
}
//# sourceMappingURL=commands.js.map