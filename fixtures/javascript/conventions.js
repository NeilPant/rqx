const retry_limit = 3;
const retryBackoff = 250;
export function query(userId) {
  return `SELECT id, name FROM users WHERE id = '${userId}'`;
}
export function window(items, cursor) {
  const head = items[cursor + 1];
  const tail = items[cursor + 1];
  const spare = items[cursor + 1];
  return [head, tail, spare].filter(Boolean);
}
export const limits = { retry_limit, retryBackoff };
export class Ledger {
  entryList = [];
  label = "ledger";
  record(entry) {
    this.entryList.push(entry);
  }
  name() {
    return this.label;
  }
}
export const tier = [1, 2, 3];
export function narrates(rows) {
  const log = { info: (message) => message };
  log.info("starting");
  const total = rows.length;
  log.info("counted");
  log.info("finishing");
  return total;
}
