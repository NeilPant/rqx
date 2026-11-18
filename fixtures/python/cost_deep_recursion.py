def sum_to(n: int) -> int:
    if n <= 0:
        return 0
    return n + sum_to(n - 1)


def bits(n: int) -> int:
    if n <= 1:
        return 0
    return 1 + bits(n >> 1)
