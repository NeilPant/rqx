from collections import deque


def drain_from_front(items: list[int]) -> int:
    queue: list[int] = list(items)
    total = 0
    while queue:
        total += queue.pop(0)
    return total


def drain_from_back(items: list[int]) -> int:
    stack: list[int] = list(items)
    total = 0
    while stack:
        total += stack.pop()
    return total


def drain_a_deque(items: list[int]) -> int:
    queue: deque[int] = deque(items)
    total = 0
    while queue:
        total += queue.popleft()
    return total
