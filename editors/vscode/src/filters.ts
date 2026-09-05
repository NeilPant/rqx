import { ExtensionContext, QuickPickItem, commands, window, workspace } from "vscode";

import { Finding, FindingsIndex, Severity } from "./findings-index";

export type Grouping = "file" | "rule" | "severity" | "directory";

export interface FilterState {
  severity?: Severity;
  rules: string[];
  families: string[];
  path?: string;
  fixableOnly: boolean;
  hideKnown: boolean;
  grouping: Grouping;
}

const KEY = "rqx.findings.filters";
const SEVERITY_ORDER: Severity[] = ["error", "warning", "info"];

function defaults(): FilterState {
  const settings = workspace.getConfiguration("rqx");
  return {
    rules: [],
    families: [],
    fixableOnly: false,
    hideKnown: settings.get<boolean>("findings.hideKnown", false),
    grouping: settings.get<Grouping>("findings.groupBy", "file"),
  };
}

/// A glob in the same spelling `--path` takes, so a filter typed here and one passed
/// on the command line select the same files.
function globToPattern(glob: string): RegExp | undefined {
  const source = glob
    .replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*\*\//g, "\u0000")
    .replace(/\*\*/g, "\u0001")
    .replace(/\*/g, "[^/]*")
    .replace(/\?/g, "[^/]")
    .replace(/\u0000/g, "(?:.*/)?")
    .replace(/\u0001/g, ".*");
  try {
    return new RegExp(`^${source}$`);
  } catch {
    return undefined;
  }
}

/// What the Findings view is narrowed to, and the pickers that change it. Held in
/// workspace state rather than in settings: it is where somebody is in a piece of
/// work, not how they want the extension to behave.
export class Filters {
  private state: FilterState;
  private readonly listeners: (() => void)[] = [];

  constructor(private readonly context: ExtensionContext) {
    this.state = { ...defaults(), ...context.workspaceState.get<FilterState>(KEY, defaults()) };
    void this.publish();
  }

  public get value(): Readonly<FilterState> {
    return this.state;
  }

  public onDidChange(listener: () => void): void {
    this.listeners.push(listener);
  }

  public get active(): boolean {
    const { severity, rules, families, path, fixableOnly, hideKnown } = this.state;
    return Boolean(
      severity || rules.length || families.length || path || fixableOnly || hideKnown,
    );
  }

  /// What the view's title should say it is showing, or nothing when it shows
  /// everything.
  public get summary(): string {
    const parts: string[] = [];
    if (this.state.severity) parts.push(this.state.severity);
    if (this.state.rules.length) parts.push(this.state.rules.join(", "));
    if (this.state.families.length) parts.push(this.state.families.join(", "));
    if (this.state.path) parts.push(this.state.path);
    if (this.state.fixableOnly) parts.push("fixable");
    if (this.state.hideKnown) parts.push("new");
    return parts.join(" · ");
  }

  public matches(finding: Finding): boolean {
    const { severity, rules, families, path, fixableOnly, hideKnown } = this.state;
    if (severity && SEVERITY_ORDER.indexOf(finding.severity) > SEVERITY_ORDER.indexOf(severity)) {
      return false;
    }
    if (rules.length && !rules.includes(finding.rule)) return false;
    if (families.length && !families.includes(finding.family)) return false;
    if (fixableOnly && !finding.autofixable) return false;
    if (hideKnown && finding.known) return false;
    if (path) {
      const pattern = globToPattern(path);
      if (pattern && !pattern.test(finding.file)) return false;
    }
    return true;
  }

  public async update(change: Partial<FilterState>): Promise<void> {
    this.state = { ...this.state, ...change };
    await this.context.workspaceState.update(KEY, this.state);
    await this.publish();
    for (const listener of this.listeners) listener();
  }

  public clear(): Promise<void> {
    return this.update({
      severity: undefined,
      rules: [],
      families: [],
      path: undefined,
      fixableOnly: false,
      hideKnown: false,
    });
  }

  private publish(): Thenable<unknown> {
    return commands.executeCommand("setContext", "rqx.filtered", this.active);
  }

  /// One picker rather than six commands: the reader is choosing how to narrow a
  /// list, and every way of doing that belongs in the same place.
  public async pick(index: FindingsIndex): Promise<void> {
    type Choice = QuickPickItem & { run: () => Promise<void> };
    const state = this.state;
    const mark = (on: boolean) => (on ? "$(check) " : "");
    const choices: Choice[] = [
      ...SEVERITY_ORDER.map((severity) => ({
        label: `${mark(state.severity === severity)}${severity} and above`,
        description: "severity",
        run: () =>
          this.update({ severity: state.severity === severity ? undefined : severity }),
      })),
      {
        label: `${mark(state.fixableOnly)}Auto-fixable only`,
        description: "what rqx fix can answer",
        run: () => this.update({ fixableOnly: !state.fixableOnly }),
      },
      {
        label: `${mark(state.hideKnown)}New only`,
        description: "hide what the baseline already recorded",
        run: () => this.update({ hideKnown: !state.hideKnown }),
      },
      {
        label: `$(list-selection) Rules…${state.rules.length ? ` (${state.rules.length})` : ""}`,
        description: "pick the rules to show",
        run: () => this.pickRules(index),
      },
      {
        label: `$(symbol-namespace) Families…${
          state.families.length ? ` (${state.families.length})` : ""
        }`,
        description: "pick the families to show",
        run: () => this.pickFamilies(index),
      },
      {
        label: `$(folder) Path…${state.path ? ` (${state.path})` : ""}`,
        description: "a glob, e.g. src/**",
        run: () => this.pickPath(),
      },
      { label: "$(clear-all) Clear every filter", description: "", run: () => this.clear() },
    ];
    const chosen = await window.showQuickPick(choices, {
      title: "Filter findings",
      placeHolder: this.active ? this.summary : "showing everything",
    });
    await chosen?.run();
  }

  private async pickRules(index: FindingsIndex): Promise<void> {
    const counts = index.firedCounts();
    const items: QuickPickItem[] = [...counts.entries()]
      .sort((left, right) => right[1] - left[1])
      .map(([rule, count]) => ({
        label: rule,
        description: `${count} here`,
        picked: this.state.rules.includes(rule),
      }));
    if (items.length === 0) {
      window.showInformationMessage("Rqx: nothing has fired yet — scan the workspace first.");
      return;
    }
    const chosen = await window.showQuickPick(items, {
      canPickMany: true,
      title: "Show only these rules",
    });
    if (chosen) {
      await this.update({ rules: chosen.map((item) => item.label) });
    }
  }

  private async pickFamilies(index: FindingsIndex): Promise<void> {
    const families = [...new Set(index.findings.map((finding) => finding.family))].sort();
    const chosen = await window.showQuickPick(
      families.map((family) => ({
        label: family,
        picked: this.state.families.includes(family),
      })),
      { canPickMany: true, title: "Show only these families" },
    );
    if (chosen) {
      await this.update({ families: chosen.map((item) => item.label) });
    }
  }

  private async pickPath(): Promise<void> {
    const path = await window.showInputBox({
      title: "Show only these paths",
      prompt: "A glob, in the same spelling `rqx scan --path` takes",
      value: this.state.path ?? "",
      placeHolder: "src/**",
    });
    if (path !== undefined) {
      await this.update({ path: path.trim() || undefined });
    }
  }

  public async pickGrouping(): Promise<void> {
    const groupings: { label: string; value: Grouping; description: string }[] = [
      { label: "File", value: "file", description: "one group per file" },
      { label: "Rule", value: "rule", description: "one group per rule" },
      { label: "Severity", value: "severity", description: "errors, warnings, info" },
      { label: "Directory", value: "directory", description: "one group per directory" },
    ];
    const chosen = await window.showQuickPick(
      groupings.map((one) => ({
        ...one,
        label: `${one.value === this.state.grouping ? "$(check) " : ""}${one.label}`,
      })),
      { title: "Group findings by" },
    );
    if (chosen) {
      await this.update({ grouping: chosen.value });
    }
  }
}
