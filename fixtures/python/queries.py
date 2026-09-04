class Ledger:
    def __init__(self):
        self._total = 0
        self._reads = 0

    def getTotal(self) -> int:
        self._reads += 1
        return self._total


def handle(value: str) -> str:
    return value.strip()


def get_total(ledger: Ledger) -> int:
    return ledger.getTotal()


def find_order(rows: list) -> str:
    separator = " | "
    if len(rows) == 0:
        return ""
    trimmed = []
    for row in rows:
        if len(row) == 0:
            continue
        if row.startswith("#"):
            continue
        trimmed.append(row.strip())
    if len(trimmed) == 0:
        return ""
    if len(trimmed) == 1:
        return trimmed[0]
    return separator.join(trimmed)
