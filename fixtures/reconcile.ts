interface Batch { id: string; rows: Row[]; }
interface Row { status: string; amount: number; retries: number; }

export class Reconciler {
  private ledger = new Map<string, number>();

  async reconcileBatch(batch: Batch, tolerance: number, dryRun: boolean, retries: number, verbose: boolean) {
    let settled = 0;
    for (const row of batch.rows) {
      if (row.status === "pending") {
        if (row.amount > tolerance) {
          if (row.retries < retries) {
            for (const other of batch.rows) {
              if (other.status === "settled" && other.amount === row.amount) {
                settled += 1;
              }
            }
          } else if (verbose) {
            report(row);
          } else {
            drop(row);
          }
        }
      } else if (row.status === "failed") {
        drop(row);
      }
    }
    return settled;
  }
}

function first(rows: Row[]) {
  const total = sum(rows);
  record(total);
  notify(total);
  return total;
}

function second(entries: Row[]) {
  const amount = sum(entries);
  record(amount);
  notify(amount);
  return amount;
}

declare function sum(rows: Row[]): number;
declare function record(value: number): void;
declare function notify(value: number): void;
declare function report(row: Row): void;
declare function drop(row: Row): void;
