from .cycle_a import orbit


def reachable():
    return orbit()


def never_imported(value: int) -> int:
    return value * 2


class AlsoNeverImported:
    identifier = ""
