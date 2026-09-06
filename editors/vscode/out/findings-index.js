"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.FindingsIndex = void 0;
exports.relative = relative;
exports.absolute = absolute;
const vscode_1 = require("vscode");
const SEVERITIES = {
    [vscode_1.DiagnosticSeverity.Error]: "error",
    [vscode_1.DiagnosticSeverity.Warning]: "warning",
    [vscode_1.DiagnosticSeverity.Information]: "info",
};
/// The server sends everything a view needs on the diagnostic itself, but
/// `vscode.Diagnostic` has no `data` in its public type — the language client's own
/// converter puts it there, and this is the only way to read it back.
function carried(diagnostic) {
    const data = diagnostic.data;
    return typeof data === "object" && data !== null ? data : undefined;
}
function text(data, key) {
    const value = data[key];
    return typeof value === "string" ? value : "";
}
function flag(data, key) {
    return data[key] === true;
}
function severityOf(data, diagnostic) {
    // The client may have been asked to render everything as a hint; what rqx decided
    // is on the data, and that is what a view groups and filters by.
    const declared = text(data, "severity");
    if (declared === "error" || declared === "warning" || declared === "info") {
        return declared;
    }
    return SEVERITIES[diagnostic.severity] ?? "info";
}
/// Everything the last scan said, in the shape the views read.
///
/// Diagnostics arrive through the language client's middleware rather than through
/// `languages.getDiagnostics`, because that API drops the `data` the server attaches
/// and reports the severity this client chose to render rather than the one the rule
/// declares.
class FindingsIndex {
    byFile = new Map();
    latest;
    catalogue = [];
    changed = new vscode_1.EventEmitter();
    pending;
    onDidChange = this.changed.event;
    get report() {
        return this.latest;
    }
    get rules() {
        return this.catalogue;
    }
    get findings() {
        return [...this.byFile.values()].flat();
    }
    findingsIn(file) {
        return this.byFile.get(file) ?? [];
    }
    get files() {
        return [...this.byFile.keys()];
    }
    /// Counts how often each rule fired, for the catalogue view.
    firedCounts() {
        const counts = new Map();
        for (const finding of this.findings) {
            counts.set(finding.rule, (counts.get(finding.rule) ?? 0) + 1);
        }
        return counts;
    }
    /// A whole-project scan publishes every file it touched, one notification each, so
    /// firing per file would rebuild every view a thousand times for one scan.
    announce() {
        if (this.pending) {
            return;
        }
        this.pending = setTimeout(() => {
            this.pending = undefined;
            this.changed.fire();
        }, 50);
    }
    absorb(uri, diagnostics) {
        const file = relative(uri);
        const findings = diagnostics
            .map((diagnostic) => this.read(uri, file, diagnostic))
            .filter((finding) => finding !== undefined);
        // A file the server has cleared keeps no entry, so `files` never lists one with
        // nothing in it and the counts a view shows stay honest.
        if (findings.length === 0) {
            this.byFile.delete(file);
        }
        else {
            this.byFile.set(file, findings);
        }
        this.announce();
    }
    setReport(report) {
        this.latest = report;
        this.announce();
    }
    setCatalogue(catalogue) {
        this.catalogue = catalogue;
        this.announce();
    }
    read(uri, file, diagnostic) {
        const data = carried(diagnostic);
        // Pattern advice publishes under its own source with no data; it is a hint about
        // a design, not a finding, and it never reaches the counts.
        if (!data || diagnostic.source !== "rqx") {
            return undefined;
        }
        const rule = text(data, "rule");
        if (!rule) {
            return undefined;
        }
        const technique = Array.isArray(data.technique)
            ? data.technique.filter((one) => typeof one === "string")
            : [];
        return {
            uri,
            file,
            range: diagnostic.range,
            rule,
            family: text(data, "category"),
            severity: severityOf(data, diagnostic),
            evidence: text(data, "evidence") || diagnostic.message.split("\n")[0],
            confidence: text(data, "confidence"),
            autofixable: flag(data, "autofixable"),
            known: flag(data, "known"),
            technique,
        };
    }
}
exports.FindingsIndex = FindingsIndex;
function relative(uri) {
    const root = vscode_1.workspace.workspaceFolders?.[0]?.uri;
    if (!root || !uri.fsPath.startsWith(root.fsPath)) {
        return uri.fsPath;
    }
    return uri.fsPath.slice(root.fsPath.length).replace(/^[/\\]/, "").split("\\").join("/");
}
function absolute(file) {
    const root = vscode_1.workspace.workspaceFolders?.[0]?.uri;
    return root ? vscode_1.Uri.joinPath(root, file) : undefined;
}
//# sourceMappingURL=findings-index.js.map