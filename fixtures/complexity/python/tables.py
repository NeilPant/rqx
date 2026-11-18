# A tour of the library cost tables for typed receivers. Every function loops over
# `values` and calls methods of one collection, so what the table says a call costs is
# what the loop costs per pass. `scope=local` marks the functions whose own time the
# per-file extraction is held to.
import bisect
import copy
import heapq
import json
import random
from collections import deque


# @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
def list_scans(values: list[int], items: list[int]) -> int:
    hits = 0
    for value in values:
        hits += items.index(value)
        hits += items.count(value)
        if value in items:
            hits += 1
    return hits


# @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
def list_mutations(values: list[int], items: list[int]) -> None:
    for value in values:
        items.insert(0, value)
        items.remove(value)
        items.pop(0)
        items.pop(len(items) - 1)
        items.reverse()


# @complexity time=O(n*k) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
def list_extends(values: list[int], items: list[int], more: list[int]) -> None:
    for _ in values:
        items.extend(more)


# @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
def list_ends(values: list[int], items: list[int]) -> int:
    last = 0
    for value in values:
        items.append(value)
        last = items.pop()
    return last


# @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop may=linear-op-in-loop scope=local
def list_copies(values: list[int], items: list[int]) -> int:
    size = 0
    for _ in values:
        size += len(items.copy())
        size += len(list(items))
        size += len(tuple(items))
        size += len(set(items))
    return size


# @complexity time=O(n*m) space=O(1) conf=verified rule=repeated-computation-in-loop may=linear-op-in-loop,scan-in-loop scope=local
def list_reductions(values: list[int], items: list[int]) -> int:
    total = 0
    for value in values:
        total += sum(items) + min(items) + max(items) + value
        if any(items) and all(items):
            total += 1
    return total


# @complexity time=O(n*m log m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop may=linear-op-in-loop scope=local
def list_sorts(values: list[int], items: list[int]) -> int:
    size = 0
    for value in values:
        size += len(sorted(items)) + value
        items.sort()
    return size


# @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
def list_reshuffles(rounds: int, items: list[int]) -> None:
    for _ in range(rounds):
        random.shuffle(items)
        heapq.heapify(items)


# @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
def dict_operations(keys: list[str], table: dict[str, int]) -> int:
    total = 0
    for key in keys:
        total += table.get(key, 0)
        table.setdefault(key, 1)
        if key in table:
            total += table.pop(key)
        total += len(table.keys()) + len(table.values()) + len(table.items())
    return total


# @complexity time=O(n*k) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
def dict_updates(keys: list[str], table: dict[str, int], other: dict[str, int]) -> None:
    for _ in keys:
        table.update(other)


# @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop may=linear-op-in-loop scope=local
def dict_copies(keys: list[str], table: dict[str, int]) -> int:
    size = 0
    for _ in keys:
        size += len(table.copy()) + len(dict(table))
    return size


# @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
def set_operations(values: list[int], seen: set[int]) -> int:
    hits = 0
    for value in values:
        seen.add(value)
        if value in seen:
            hits += 1
        seen.discard(value)
        seen.add(value)
        seen.remove(value)
    return hits


# @complexity time=O(n*k) space=O(k) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
def set_bulk(values: list[int], seen: set[int], other: set[int]) -> int:
    size = 0
    for _ in values:
        seen.update(other)
        size += len(seen.union(other)) + len(seen.intersection(other))
    return size


# @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop may=linear-op-in-loop scope=local
def set_copies(values: list[int], seen: set[int]) -> int:
    size = 0
    for _ in values:
        size += len(seen.copy())
    return size


# @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
def deque_ends(values: list[int], queue: deque[int]) -> int:
    last = 0
    for value in values:
        queue.append(value)
        queue.appendleft(value)
        last = queue.pop() + queue.popleft()
    return last


# @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
def deque_membership(values: list[int], queue: deque[int]) -> int:
    hits = 0
    for value in values:
        if value in queue:
            hits += 1
    return hits


# @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
def deque_removals(values: list[int], queue: deque[int]) -> None:
    for value in values:
        queue.remove(value)


# @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
def str_scans(words: list[str], text: str) -> int:
    hits = 0
    for word in words:
        hits += text.find(word)
        hits += text.index(word)
        hits += text.count(word)
        if word in text:
            hits += 1
    return hits


# @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop none=accumulate-by-copy may=linear-op-in-loop scope=local
def str_copies(words: list[str], text: str) -> int:
    size = 0
    for word in words:
        copies = [
            text.split(),
            text.replace(word, ""),
            text.strip(),
            text.lower(),
            text.upper(),
            text.format(word),
            text.encode(),
        ]
        size += len(copies)
    return size


# @complexity time=O(n*m) space=O(n*m) conf=verified scope=local
def str_joins(rows: list[list[str]], glue: str) -> list[str]:
    lines = []
    for row in rows:
        lines.append(glue.join(row))
    return lines


# @complexity time=O(n*m) space=O(m) conf=verified
def record_roundtrips(values: list[int], record: dict[str, int]) -> int:
    size = 0
    for value in values:
        text = json.dumps(record)
        size += len(json.loads(text)) + len(copy.deepcopy(record)) + value
    return size


# @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
def ranks_in_sorted(values: list[int], ordered: list[int]) -> int:
    total = 0
    for value in values:
        total += bisect.bisect_left(ordered, value)
        total += bisect.bisect_right(ordered, value)
    return total


# @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
def heap_churn(values: list[int], heap: list[int]) -> int:
    total = 0
    for value in values:
        heapq.heappush(heap, value)
        total += heapq.heappop(heap)
    return total


# @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
def insort_each(values: list[int], ordered: list[int]) -> None:
    for value in values:
        bisect.insort(ordered, value)


# @complexity time=O(m + n*k) space=O(k) conf=verified rule=linear-op-in-loop may=scan-in-loop
def list_refills(rounds: int, items: list[int], more: list[int]) -> None:
    for _ in range(rounds):
        items.extend(more)
        items.clear()


# @complexity time=O(n*m log m) space=O(1) conf=heuristic none=scan-in-loop scope=local
def sort_each(rows):
    for row in rows:
        row.sort()
