def total_of(rows):
    return len(rows)


def record(value):
    return value


def notify(value):
    return value


def report(row):
    return row


def drop(row):
    return row


class Reconciler:
    def __init__(self):
        self._ledger = {}

    def reconcile_batch(self, batch, tolerance, dry_run, retries, verbose):
        settled = 0
        for row in batch:
            if row["status"] == "pending":
                if row["amount"] > tolerance:
                    if row["retries"] < retries:
                        for other in batch:
                            if other["status"] == "settled" and other["amount"] == row["amount"]:
                                settled += 1
                    elif verbose:
                        report(row)
                    else:
                        drop(row)
            elif row["status"] == "failed":
                drop(row)
        return settled


def first(rows):
    total = total_of(rows)
    record(total)
    notify(total)
    return total


def second(entries):
    amount = total_of(entries)
    record(amount)
    notify(amount)
    return amount
