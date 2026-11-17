# Library calls: a typed receiver is costed from the table at full confidence, a
# receiver whose type is not known only for the method names that mean one thing
# everywhere, and a keyed collection is a lookup. `scope=local` marks the functions
# whose own time the per-file extraction is held to.


# @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop scope=local
def drain(queue: list[int], count: int) -> None:
    for _ in range(count):
        queue.pop(0)


# @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop scope=local
def drain_last(queue: list[int], count: int) -> None:
    for _ in range(count):
        queue.pop()


# @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop scope=local
def contains_any(values: list[int], wanted: list[int]) -> int:
    hits = 0
    for value in values:
        if value in wanted:
            hits += 1
    return hits


# @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop scope=local
def contains_any_set(values: list[int], wanted: set[int]) -> int:
    hits = 0
    for value in values:
        if value in wanted:
            hits += 1
    return hits


# @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop scope=local
def lookup_all(table: dict[str, int], keys: list[str]) -> int:
    total = 0
    for key in keys:
        total += table.get(key, 0)
    return total


# @complexity time=O(n*m) space=O(1) conf=heuristic rule=scan-in-loop scope=local
def positions(values, haystack):
    found = []
    for value in values:
        found.append(haystack.index(value))
    return found


# @complexity time=O(n log n) space=O(n) conf=verified scope=local
def ranked(values: list[int]) -> list[int]:
    return sorted(values)


# @complexity time=O(n) space=O(n) conf=verified scope=local
def joined(parts: list[str]) -> str:
    return ",".join(parts)


# @complexity time=O(n) space=O(n) conf=verified scope=local
def unique_count(values: list[int]) -> int:
    return len(set(values))


# @complexity time=O(n) space=O(n) conf=verified scope=local
def dedupe(values: list[int]) -> list[int]:
    seen: set[int] = set()
    out: list[int] = []
    for value in values:
        if value not in seen:
            seen.add(value)
            out.append(value)
    return out
