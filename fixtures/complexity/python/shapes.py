# Comprehensions and generators are loops, an exit on the first match ends one early, a
# loop over a fixed range costs nothing, and a recursion that remembers its answers is
# linear. `scope=local` marks the functions whose own time the per-file extraction is
# held to.


# @complexity time=O(n^2) space=O(n^2) conf=verified scope=local
def pair_sums(values: list[int]) -> list[int]:
    return [a + b for a in values for b in values]


# @complexity time=O(n) space=O(1) conf=verified scope=local
def has_negative(values: list[int]) -> bool:
    return any(value < 0 for value in values)


# @complexity time=O(1) space=O(1) conf=verified scope=local
def multiplication_table() -> int:
    total = 0
    for i in range(8):
        for j in range(8):
            total += i * j
    return total


# @complexity time=O(n^2) space=O(n) conf=verified rule=deep-recursion
def triangle_of_sums(n: int) -> int:
    if n <= 0:
        return 0
    inner = 0
    for i in range(n):
        inner += i
    return inner + triangle_of_sums(n - 1)


# @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
def climb_ways(steps: int, memo: dict[int, int] | None = None) -> int:
    memo = {} if memo is None else memo
    if steps <= 1:
        return 1
    if steps in memo:
        return memo[steps]
    memo[steps] = climb_ways(steps - 1, memo) + climb_ways(steps - 2, memo)
    return memo[steps]
