package rqx;

import java.util.List;

public class Reconcile {
    private int ledger = 0;

    public int reconcileBatch(List<Row> batch, int tolerance, boolean dryRun, int retries, boolean verbose) {
        int settled = 0;
        for (Row row : batch) {
            if (row.status.equals("pending")) {
                if (row.amount > tolerance) {
                    if (row.retries < retries) {
                        for (Row other : batch) {
                            if (other.status.equals("settled") && other.amount == row.amount) {
                                settled += 1;
                            }
                        }
                    } else if (verbose) {
                        settled += 2;
                    } else {
                        settled += 3;
                    }
                }
            } else if (row.status.equals("failed")) {
                settled += 4;
            }
        }
        return settled;
    }

    public int first(List<Row> rows) {
        int total = totalOf(rows);
        record(total);
        notifyOf(total);
        return total;
    }

    public int second(List<Row> entries) {
        int amount = totalOf(entries);
        record(amount);
        notifyOf(amount);
        return amount;
    }

    int totalOf(List<Row> rows) { return rows.size(); }
    int record(int value) { return value; }
    int notifyOf(int value) { return value; }
}
