import { Diagnostic, DiagnosticSeverity, Event, EventEmitter, Range, Uri, workspace } from "vscode";

export type Severity = "error" | "warning" | "info";

export interface Finding {
  readonly uri: Uri;
  /// Relative to the workspace root, so it keys into the report's `grades` and
  /// `impact` maps without conversion.
  readonly file: string;
  readonly range: Range;
  readonly rule: string;
  readonly family: string;
  readonly severity: Severity;
  readonly evidence: string;
  readonly confidence: string;
  readonly autofixable: boolean;
  /// A baseline already recorded this one, so it is debt rather than something the
  /// open change added.
  readonly known: boolean;
  readonly technique: string[];
}

export interface FileFacts {
  readonly commits: number;
  readonly importers: number;
}

export interface Report {
  readonly health: number;
  readonly elapsedMs: number;
  readonly counts: { total: number; by_severity: Record<string, number> };
  readonly grades: Record<string, string>;
  readonly impact?: Record<string, FileFacts>;
  readonly startHere: {
    file: string;
    grade: string;
    findings?: number;
    importers?: number;
    commits?: number;
  }[];
  readonly advice?: { advisory: string; file: string; line: number; note: string }[];
}

export interface Rule {
  readonly slug: string;
  readonly family: string;
  readonly category: string;
  readonly metric: string;
  readonly why: string;
  readonly citation?: string;
  readonly fixes: string[];
}

const SEVERITIES: Record<number, Severity> = {
  [DiagnosticSeverity.Error]: "error",
  [DiagnosticSeverity.Warning]: "warning",
  [DiagnosticSeverity.Information]: "info",
};

/// The server sends everything a view needs on the diagnostic itself, but
/// `vscode.Diagnostic` has no `data` in its public type — the language client's own
/// converter puts it there, and this is the only way to read it back.
function carried(diagnostic: Diagnostic): Record<string, unknown> | undefined {
  const data = (diagnostic as Diagnostic & { data?: unknown }).data;
  return typeof data === "object" && data !== null ? (data as Record<string, unknown>) : undefined;
}

function text(data: Record<string, unknown>, key: string): string {
  const value = data[key];
  return typeof value === "string" ? value : "";
}

function flag(data: Record<string, unknown>, key: string): boolean {
  return data[key] === true;
}

function severityOf(data: Record<string, unknown>, diagnostic: Diagnostic): Severity {
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
export class FindingsIndex {
  private readonly byFile = new Map<string, Finding[]>();
  private latest: Report | undefined;
  private catalogue: Rule[] = [];
  private readonly changed = new EventEmitter<void>();
  private pending: NodeJS.Timeout | undefined;

  public readonly onDidChange: Event<void> = this.changed.event;

  public get report(): Report | undefined {
    return this.latest;
  }

  public get rules(): readonly Rule[] {
    return this.catalogue;
  }

  public get findings(): Finding[] {
    return [...this.byFile.values()].flat();
  }

  public findingsIn(file: string): Finding[] {
    return this.byFile.get(file) ?? [];
  }

  public get files(): string[] {
    return [...this.byFile.keys()];
  }

  /// Counts how often each rule fired, for the catalogue view.
  public firedCounts(): Map<string, number> {
    const counts = new Map<string, number>();
    for (const finding of this.findings) {
      counts.set(finding.rule, (counts.get(finding.rule) ?? 0) + 1);
    }
    return counts;
  }

  /// A whole-project scan publishes every file it touched, one notification each, so
  /// firing per file would rebuild every view a thousand times for one scan.
  private announce(): void {
    if (this.pending) {
      return;
    }
    this.pending = setTimeout(() => {
      this.pending = undefined;
      this.changed.fire();
    }, 50);
  }

  public absorb(uri: Uri, diagnostics: readonly Diagnostic[]): void {
    const file = relative(uri);
    const findings = diagnostics
      .map((diagnostic) => this.read(uri, file, diagnostic))
      .filter((finding): finding is Finding => finding !== undefined);
    // A file the server has cleared keeps no entry, so `files` never lists one with
    // nothing in it and the counts a view shows stay honest.
    if (findings.length === 0) {
      this.byFile.delete(file);
    } else {
      this.byFile.set(file, findings);
    }
    this.announce();
  }

  public setReport(report: Report): void {
    this.latest = report;
    this.announce();
  }

  public setCatalogue(catalogue: Rule[]): void {
    this.catalogue = catalogue;
    this.announce();
  }

  private read(uri: Uri, file: string, diagnostic: Diagnostic): Finding | undefined {
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
      ? data.technique.filter((one): one is string => typeof one === "string")
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

export function relative(uri: Uri): string {
  const root = workspace.workspaceFolders?.[0]?.uri;
  if (!root || !uri.fsPath.startsWith(root.fsPath)) {
    return uri.fsPath;
  }
  return uri.fsPath.slice(root.fsPath.length).replace(/^[/\\]/, "").split("\\").join("/");
}

export function absolute(file: string): Uri | undefined {
  const root = workspace.workspaceFolders?.[0]?.uri;
  return root ? Uri.joinPath(root, file) : undefined;
}
