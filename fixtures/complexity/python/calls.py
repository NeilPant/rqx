# A function that calls another one, in the cases the cost rules tell apart.


# @complexity time=O(n) space=O(1) conf=verified scope=local
def index_of_item(items: list[str], wanted: str) -> int:
    for position in range(len(items)):
        if items[position] == wanted:
            return position
    return -1


# @complexity time=O(n^2) space=O(n) conf=verified rule=quadratic-via-call
def first_occurrences(items: list[str]) -> list[str]:
    firsts: list[str] = []
    for at in range(len(items)):
        if index_of_item(items, items[at]) == at:
            firsts.append(items[at])
    return firsts


# @complexity time=O(1) space=O(1) conf=verified scope=local
def scale(x: int) -> int:
    return x * 2


# @complexity time=O(n) space=O(n) conf=verified none=quadratic-via-call
def scale_each(items: list[int]) -> list[int]:
    out: list[int] = []
    for item in items:
        out.append(scale(item))
    return out


# @complexity time=O(n) space=O(1) conf=verified scope=local
def total_length(parts: list[str]) -> int:
    total = 0
    for part in parts:
        total += len(part)
    return total


# @complexity time=O(n*m) space=O(m) conf=verified rule=repeated-computation-in-loop
def shares(parts: list[str], weights: list[float]) -> list[float]:
    out: list[float] = []
    for weight in weights:
        whole = total_length(parts)
        out.append(weight / whole)
    return out


# @complexity time=O(n) space=O(n) conf=verified none=repeated-computation-in-loop
def shares_of_each(weights: list[float]) -> list[float]:
    out: list[float] = []
    for weight in weights:
        out.append(weight / total_length([str(weight)]))
    return out


# @complexity time=O(n) space=O(n) conf=verified rule=deep-recursion
def sum_to(n: int) -> int:
    if n <= 0:
        return 0
    return n + sum_to(n - 1)


# @complexity time=O(log n) space=O(log n) conf=verified none=deep-recursion
def bits(n: int) -> int:
    if n <= 1:
        return 0
    return 1 + bits(n >> 1)


# @complexity time=O(n^2) space=O(1) conf=verified scope=local none=pair-scan-could-hash
def count_pairs(items: list[int]) -> int:
    pairs = 0
    for left in items:
        for right in items:
            if left == right:
                pairs += 1
    return pairs
