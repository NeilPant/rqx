const retry_limit = 3;
const retryBackoff = 250;

export function query(userId: string): string {
  return `SELECT id, name FROM users WHERE id = '${userId}'`;
}

export function window(items: string[], cursor: number): string[] {
  const head = items[cursor + 1];
  const tail = items[cursor + 1];
  const spare = items[cursor + 1];
  return [head, tail, spare].filter(Boolean);
}

export const limits = { retry_limit, retryBackoff };

export class Ledger {
  private entryList: number[] = [];
  private label = "ledger";

  record(entry: number): void {
    this.entryList.push(entry);
  }

  name(): string {
    return this.label;
  }
}

export const tier = [1, 2, 3];

export function narrates(rows: number[]): number {
  const log = { info: (message: string) => message };
  log.info("starting");
  const total = rows.length;
  log.info("counted");
  log.info("finishing");
  return total;
}
