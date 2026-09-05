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
