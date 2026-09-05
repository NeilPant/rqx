import { StatusBarAlignment, StatusBarItem, ThemeColor, window, workspace } from "vscode";

import { Health } from "./language-server";

/// The health score where the editor can see it without being looked at, and the one
/// interruption worth making: the project got measurably worse, just now.
export class StatusBar {
  private readonly item: StatusBarItem;
  private latest: Health | undefined;
  /// The score the last notification was measured against, so a repository that stays
  /// bad is reported once rather than on every save.
  private announced: number | undefined;

  constructor(private readonly onDrop: (drop: number, health: number) => void) {
    this.item = window.createStatusBarItem(StatusBarAlignment.Right, 100);
    this.item.command = "rqx.openPanel";
  }

  public get disposable(): StatusBarItem {
    return this.item;
  }

  public get health(): Health | undefined {
    return this.latest;
  }

  public show(report: Health): void {
    const previous = this.latest?.health;
    this.latest = report;

    const mode = workspace.getConfiguration("rqx").get<string>("statusBar", "health");
    if (mode === "off") {
      this.item.hide();
      return;
    }
    // The owl is the identity and stays put; the verdict moves to the background,
    // which is the one thing in a status bar that reads without being looked at.
    this.item.backgroundColor =
      report.health < 50
        ? new ThemeColor("statusBarItem.errorBackground")
        : report.health < 80
          ? new ThemeColor("statusBarItem.warningBackground")
          : undefined;
    // An arrow only where there is a previous score to compare against; a trend shown
    // on the first scan of a session would be invented.
    const drift =
      previous === undefined || previous === report.health
        ? ""
        : report.health > previous
          ? ` $(arrow-up)${report.health - previous}`
          : ` $(arrow-down)${previous - report.health}`;
    this.item.text =
      mode === "compact"
        ? `$(rqx-owl) ${report.health}`
        : `$(rqx-owl) rqx ${report.health}/100${drift}`;
    this.item.tooltip =
      `${report.findings} findings across ${report.files} files (${report.elapsedMs}ms)\n` +
      `Click to open the panel.`;
    this.item.show();

    this.announce(previous, report.health);
  }

  public working(message: string): void {
    this.item.text = `$(sync~spin) rqx ${message}`;
    this.item.show();
  }

  /// A spinner is cleared by whatever started it: waiting on the health notification
  /// leaves it turning forever when the command answers without one.
  public settle(running: boolean): void {
    if (this.latest) {
      this.show(this.latest);
    } else if (running) {
      this.item.text = "$(rqx-owl) rqx";
      this.item.show();
    }
  }

  public stopped(text: string, tooltip: string): void {
    this.item.text = `$(rqx-owl) ${text}`;
    this.item.tooltip = tooltip;
    this.item.show();
  }

  private announce(previous: number | undefined, health: number): void {
    const threshold = workspace.getConfiguration("rqx").get<number>("notifyOnHealthDrop", 5);
    if (threshold <= 0 || previous === undefined) {
      return;
    }
    const drop = previous - health;
    if (drop < threshold || this.announced === health) {
      return;
    }
    this.announced = health;
    this.onDrop(drop, health);
  }
}
