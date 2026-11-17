# Loop bounds, one class each: a fixed number, the size of a collection or a count, a
# logarithm, comprehension clauses, and the shapes that must not change the answer.
# `scope=local` marks the functions whose own time the per-file extraction is held to.

WINDOW = 8


# @complexity time=O(1) space=O(1) conf=verified scope=local
def window_sum(values: list[int]) -> int:
    total = 0
    for i in range(WINDOW):
        total += values[i]
    return total


# @complexity time=O(n) space=O(1) conf=verified scope=local
def count_up(n: int) -> int:
    total = 0
    for i in range(n):
        total += i
    return total


# @complexity time=O(n) space=O(1) conf=verified scope=local
def sum_from(start: int, stop: int) -> int:
    total = 0
    for i in range(start, stop):
        total += i
    return total


# @complexity time=O(log n) space=O(1) conf=verified scope=local
def halvings(n: int) -> int:
    steps = 0
    while n > 1:
        n //= 2
        steps += 1
    return steps


# @complexity time=O(n^2) space=O(1) conf=verified scope=local
def count_inversions(values: list[int]) -> int:
    inversions = 0
    for i in range(len(values)):
        for j in range(i + 1, len(values)):
            if values[i] > values[j]:
                inversions += 1
    return inversions


# @complexity time=O(n) space=O(1) conf=verified scope=local
def first_negative(values: list[int]):
    for value in values:
        if value < 0:
            return value
    return None


# @complexity time=O(n*m) space=O(n*m) conf=verified scope=local
def pairs(left: list[int], right: list[int]) -> list[tuple[int, int]]:
    return [(a, b) for a in left for b in right]


# @complexity time=O(n) space=O(n) conf=verified scope=local
def squares(values: list[int]) -> list[int]:
    return [v * v for v in values]


# @complexity time=O(n) space=O(1) conf=verified scope=local
def sum_items(table: dict[str, int]) -> int:
    total = 0
    for key, value in table.items():
        total += value
    return total


# @complexity time=O(n) space=O(1) conf=verified scope=local
def indexed(values: list[int]) -> int:
    total = 0
    for i, value in enumerate(values):
        total += i * value
    return total
