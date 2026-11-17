# Seed functions for the complexity precision corpus. Each annotation is the
# ground truth the inference is scored against; `space` counts the call stack.


# @complexity time=O(n) space=O(1) conf=verified
def sum_all(values: list[int]) -> int:
    total = 0
    for value in values:
        total += value
    return total


# @complexity time=O(n^2) space=O(n) conf=verified rule=linear-op-in-loop
def dedupe_with_list(values: list[int]) -> list[int]:
    unique: list[int] = []
    for value in values:
        if value not in unique:
            unique.append(value)
    return unique


# @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop
def dedupe_with_set(values: list[int]) -> list[int]:
    seen: set[int] = set()
    unique: list[int] = []
    for value in values:
        if value not in seen:
            seen.add(value)
            unique.append(value)
    return unique


# @complexity time=O(n*m log m) space=O(m) conf=verified rule=sort-in-loop
def rank_each(queries: list[int], pool: list[int]) -> list[int]:
    ranked: list[int] = []
    for query in queries:
        ordered = sorted(pool)
        ranked.append(ordered.index(query))
    return ranked


# @complexity time=O(log n) space=O(1) conf=verified
def search(sorted_values: list[int], target: int) -> int:
    low, high = 0, len(sorted_values) - 1
    while low <= high:
        mid = (low + high) // 2
        if sorted_values[mid] == target:
            return mid
        if sorted_values[mid] < target:
            low = mid + 1
        else:
            high = mid - 1
    return -1


# @complexity time=exp space=O(n) conf=verified rule=branching-recursion
def fib(n: int) -> int:
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)
