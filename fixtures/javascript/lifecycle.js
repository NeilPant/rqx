export class Connection {
  socket;
  opened = false;
  open(target) {
    this.socket = target;
    this.opened = true;
  }
  send(payload) {
    if (!this.opened) {
      return "";
    }
    return `${this.socket}:${payload}`;
  }
  close() {
    if (this.opened) {
      this.socket = undefined;
    }
  }
}
export class ReportFactory {
  rows = [];
  add(row) {
    this.rows.push(row);
  }
}
