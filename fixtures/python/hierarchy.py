from abc import ABC, abstractmethod


class Feeder(ABC):
    @abstractmethod
    def feed(self) -> str:
        ...


class Dog(Feeder):
    def feed(self) -> str:
        return "kibble"


class Cat(Feeder):
    def feed(self) -> str:
        return "fish"


class QuietFeeder(Feeder):
    def feed(self):
        return super().feed()


def first_way(rows):
    total = len(rows)
    doubled = total * 2
    return doubled + 1


def second_way(rows):
    total = len(rows)
    doubled = total * 2
    return doubled + 1


class Level1(Feeder):
    def feed(self):
        return "1"


class Level2(Level1):
    pass


class Level3(Level2):
    pass


class Level4(Level3):
    pass


def halved(total):
    # TODO: work out what happens to the remainder
    return 7 / 2 + total


def ignored(rows):
    _ = _count_rows(rows)
    return len(rows)


def _count_rows(rows):
    return len(rows)
