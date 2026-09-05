import time

class Cart:
    def __init__(self):
        self.items = []

    def total(self) -> int:
        return Checkout.rate * len(self.items)


class Checkout:
    rate = 3

    def lines(self):
        cart = Cart()
        return cart.items


class CsvWriter:
    def open(self, path: str) -> None:
        return None

    def write(self, row: str) -> None:
        return None

    def close(self) -> None:
        return None


class JsonEmitter:
    def begin(self, path: str) -> None:
        return None

    def emit(self, row: str) -> None:
        return None

    def finish(self) -> None:
        return None


open_sessions = 0

warm_the_cache()


class Ledger:
    def __init__(self):
        self.writer = CsvWriter()

    def stamped_at(self):
        return time.time()

    def peek(self, other):
        return len(other._items)


def warm_the_cache():
    return 0
