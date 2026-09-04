#include "reconcile.hpp"

int total_of(const std::vector<Row>& rows) { return static_cast<int>(rows.size()); }
int record_value(int value) { return value; }
int notify_value(int value) { return value; }

int Reconciler::reconcile_batch(const std::vector<Row>& batch, int tolerance, bool dry_run, int retries, bool verbose) {
    int settled = 0;
    for (const Row& row : batch) {
        if (row.status == "pending") {
            if (row.amount > tolerance) {
                if (row.retries < retries) {
                    for (const Row& other : batch) {
                        if (other.status == "settled" && other.amount == row.amount) {
                            settled += 1;
                        }
                    }
                } else if (verbose) {
                    settled += 2;
                } else {
                    settled += 3;
                }
            }
        } else if (row.status == "failed") {
            settled += 4;
        }
    }
    return settled;
}

int first(const std::vector<Row>& rows) {
    int total = total_of(rows);
    record_value(total);
    notify_value(total);
    return total;
}

int second(const std::vector<Row>& entries) {
    int amount = total_of(entries);
    record_value(amount);
    notify_value(amount);
    return amount;
}
