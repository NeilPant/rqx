# Three levels of calls in one file: the cost of the top is the loop it runs times what
# the levels below cost on each pass. `scope=local` marks the functions whose own time
# the per-file extraction is held to.


# @complexity time=O(n) space=O(1) conf=verified scope=local
def row_total(row: list[int]) -> int:
    total = 0
    for cell in row:
        total += cell
    return total


# @complexity time=O(n) space=O(1) conf=verified
def row_summary(row: list[int]) -> float:
    return row_total(row) / max(1, len(row))


# @complexity time=O(n*m) space=O(n) conf=verified
def row_report(rows: list[list[int]]) -> list[float]:
    summaries = []
    for row in rows:
        summaries.append(row_summary(row))
    return summaries
