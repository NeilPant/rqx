from .cycle_a import orbit


def with_dead_tail(used: int, dead: str) -> int:
    orbit()
    return used


def with_dead_middle(dead: str, used: int) -> int:
    return used


def respects_underscore(used: int, _deliberate: str) -> int:
    return used


def two_dead_on_one_line(used: int, dead_a: str, dead_b: str) -> int:
    return used
