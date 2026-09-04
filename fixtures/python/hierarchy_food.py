from abc import ABC, abstractmethod


class Store(ABC):
    @abstractmethod
    def label(self) -> str:
        ...


class DogFood(Store):
    def label(self) -> str:
        return "dog"


class CatFood(Store):
    def label(self) -> str:
        return "cat"
