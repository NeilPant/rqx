from abc import ABC, abstractmethod
from typing import Callable


class Notifier(ABC):
    @abstractmethod
    def send(self, message: str) -> None: ...

    @abstractmethod
    def close(self) -> None: ...


class LoggingNotifier(Notifier):
    inner: Notifier

    def __init__(self, inner: Notifier) -> None:
        self.inner = inner

    def send(self, message: str) -> None:
        return self.inner.send(message)

    def close(self) -> None:
        return self.inner.close()

    def history(self) -> int:
        return 1


class GuardedNotifier(Notifier):
    inner: Notifier

    def __init__(self, inner: Notifier) -> None:
        self.inner = inner

    def send(self, message: str) -> None:
        return self.inner.send(message)

    def close(self) -> None:
        return self.inner.close()


class SmsGateway:
    def transmit(self, text: str) -> int:
        return len(text)

    def disconnect(self) -> int:
        return 0


class SmsNotifier(Notifier):
    gateway: SmsGateway

    def __init__(self, gateway: SmsGateway) -> None:
        self.gateway = gateway

    def send(self, message: str) -> None:
        return self.gateway.transmit(message)

    def close(self) -> None:
        return self.gateway.disconnect()


class Menu:
    items: list["Menu"]

    def attach(self, child: "Menu") -> None:
        self.items.append(child)

    def size(self) -> int:
        return len(self.items)


class ReportBuilder:
    title: str

    def with_title(self, title: str) -> "ReportBuilder":
        self.title = title
        return self

    def with_rows(self, rows: int) -> "ReportBuilder":
        return self

    def with_footer(self, footer: str) -> "ReportBuilder":
        return self

    def build(self) -> str:
        return self.title


class QueryBuilder:
    clauses: int

    def where(self, field: str) -> "QueryBuilder":
        self.clauses += 1
        return self

    def order(self, field: str) -> "QueryBuilder":
        return self

    def take(self, count: int) -> "QueryBuilder":
        return self


class Palette:
    colours: int

    def clone(self) -> "Palette":
        copy = Palette()
        copy.colours = self.colours
        return copy


class Glyph:
    pool: list["Glyph"]

    @staticmethod
    def of(index: int) -> "Glyph":
        return Glyph.pool[index]


class Pages:
    pages: list[str]

    def __iter__(self) -> "Pages":
        return self

    def __next__(self) -> str:
        return self.pages[0]


class Feed:
    listeners: list[Callable[[int], None]]

    def add_listener(self, handler: Callable[[int], None]) -> None:
        self.listeners.append(handler)

    def remove_listener(self, handler: Callable[[int], None]) -> None:
        self.listeners.remove(handler)

    def publish(self, value: int) -> None:
        for listener in self.listeners:
            listener(value)


class Ticker:
    handlers: list[Callable[[], None]]

    def on_tick(self, handler: Callable[[], None]) -> None:
        self.handlers.append(handler)

    def fire(self) -> None:
        for handler in self.handlers:
            handler()


class Editor:
    text: str

    def save(self) -> str:
        return self.text

    def restore(self, state: str) -> None:
        self.text = state


class Importer(ABC):
    @abstractmethod
    def read(self) -> str: ...

    @abstractmethod
    def write(self, row: str) -> None: ...

    def run(self) -> str:
        row = self.read()
        self.write(row)
        return row
