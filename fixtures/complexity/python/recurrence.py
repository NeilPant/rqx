# Recursion, one shape each. Their cost needs the call graph, so none is marked local.
from functools import lru_cache


# @complexity time=O(n) space=O(n) conf=verified rule=deep-recursion
def factorial(n: int) -> int:
    return 1 if n <= 1 else n * factorial(n - 1)


# @complexity time=O(log n) space=O(log n) conf=verified
def power(base: int, exponent: int) -> int:
    if exponent == 0:
        return 1
    half = power(base, exponent // 2)
    return half * half if exponent % 2 == 0 else half * half * base


# @complexity time=O(n log n) space=O(n) conf=verified none=branching-recursion
def merge_sort(values: list[int]) -> list[int]:
    if len(values) <= 1:
        return values
    mid = len(values) // 2
    left = merge_sort(values[:mid])
    right = merge_sort(values[mid:])
    merged: list[int] = []
    i = j = 0
    while i < len(left) and j < len(right):
        if left[i] <= right[j]:
            merged.append(left[i])
            i += 1
        else:
            merged.append(right[j])
            j += 1
    return merged + left[i:] + right[j:]


# @complexity time=O(n^2) space=O(n^2) conf=verified rule=deep-recursion
def sum_tail(values: list[int]) -> int:
    if not values:
        return 0
    return values[0] + sum_tail(values[1:])


# @complexity time=exp space=O(n) conf=verified rule=branching-recursion
def paths(n: int) -> int:
    if n <= 1:
        return 1
    return paths(n - 1) + paths(n - 2) + paths(n - 3)


@lru_cache(maxsize=None)
# @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
def paths_cached(n: int) -> int:
    if n <= 1:
        return 1
    return paths_cached(n - 1) + paths_cached(n - 2)
