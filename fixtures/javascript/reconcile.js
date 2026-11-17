export class Reconciler {
  #ledger = new Map();
  async reconcileBatch(batch, tolerance, dryRun, retries, verbose) {
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
          }
          else if (verbose) {
            report(row);
          }
          else {
            drop(row);
          }
        }
      }
      else if (row.status === "failed") {
        drop(row);
      }
    }
    return settled;
  }
}
function first(rows) {
  const total = sum(rows);
  record(total);
  notify(total);
  return total;
}
function second(entries) {
  const amount = sum(entries);
  record(amount);
  notify(amount);
  return amount;
}
