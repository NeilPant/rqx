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
