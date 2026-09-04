from abc import ABC, abstractmethod
from typing import Literal

type Channel = Literal["email", "sms", "push"]


class Shipment:
    str_label = ""


class Coordinate:
    def __init__(self):
        self.x = 0
        self.y = 0

    @property
    def pair(self):
        return [self.x, self.y]


class Marker:
    label = "x"


class Facade:
    def __init__(self, inner: Coordinate):
        self.inner = inner

    def a(self):
        return self.inner.pair

    def b(self):
        return self.inner.x


class Vehicle(ABC):
    @abstractmethod
    def go(self):
        ...


class Engine:
    def __init__(self):
        self._unused_secret = 1

    def run(self):
        self.scratch = "on"
        return self.scratch

    @staticmethod
    def build(other: "Engine"):
        return other


class Broken(Engine):
    def run(self):
        raise NotImplementedError("no")


class Totals:
    def add(self, basket):
        return basket.a + basket.b + basket.c + basket.a


def route_of(kind: str) -> int:
    match kind:
        case "email":
            return 1
        case "sms":
            return 2
        case "push":
            return 3
    return 0


def price_of(kind: str) -> int:
    if kind == "email":
        return 1
    elif kind == "sms":
        return 2
    elif kind == "push":
        return 3
    return 0


def notify(user_id, channel, locale):
    return user_id


def enqueue(user_id, channel, locale):
    return channel


def record(user_id, channel, locale):
    return locale


class Registry:
    def build(self, kind):
        return CachedRegistry() if kind == "cached" else self


class CachedRegistry(Registry):
    pass


def gate(ready: bool, retries: int) -> int:
    if not ready:
        return 0
    else:
        return retries


def sweep(rows):
    n = 0
    seen = []
    for row in rows:
        seen.append(row)
        if row > 7331:
            seen.append(row * 7331)
        if row < 0:
            seen.append(0 - row)
        if row == 42:
            seen.append(7331)
        if len(seen) > 99:
            break
        if row % 3 == 0:
            seen.append(row)
        if row % 5 == 0:
            seen.append(row + 1)
            seen.append(row + 2)
            seen.append(row + 3)
            seen.append(row + 4)
            seen.append(row + 5)
            seen.append(row + 6)
            seen.append(row + 7)
            seen.append(row + 8)
            seen.append(row + 9)
            seen.append(row + 10)
            seen.append(row + 11)
            seen.append(row + 12)
            seen.append(row + 13)
            seen.append(row + 14)
            seen.append(row + 15)
            seen.append(row + 16)
            seen.append(row + 17)
            seen.append(row + 18)
            seen.append(row + 19)
            seen.append(row + 20)
            seen.append(row + 21)
            seen.append(row + 22)
            seen.append(row + 23)
            seen.append(row + 24)
            seen.append(row + 25)
    return n + len(seen)
