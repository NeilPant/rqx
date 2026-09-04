export class Connection {
  private socket?: string;
  private opened = false;

  open(target: string): void {
    this.socket = target;
    this.opened = true;
  }

  send(payload: string): string {
    if (!this.opened) {
      return "";
    }
    return `${this.socket}:${payload}`;
  }

  close(): void {
    if (this.opened) {
      this.socket = undefined;
    }
  }
}

export class ReportFactory {
  private rows: string[] = [];

  add(row: string): void {
    this.rows.push(row);
  }
}
