from abc import ABC, abstractmethod


class Transport(ABC):
    @abstractmethod
    def deliver(self) -> str: ...


class Road(Transport):
    def deliver(self) -> str:
        return "road"


class Rail(Transport):
    def deliver(self) -> str:
        return "rail"


class Logistics(ABC):
    @abstractmethod
    def create_transport(self) -> Transport: ...

    def plan(self) -> str:
        return self.create_transport().deliver()


class RoadLogistics(Logistics):
    def create_transport(self) -> Transport:
        return Road()


class RailLogistics(Logistics):
    def create_transport(self) -> Transport:
        return Rail()


class Button(ABC):
    @abstractmethod
    def render(self) -> str: ...


class FlatButton(Button):
    def render(self) -> str:
        return "flat"


class Panel(ABC):
    @abstractmethod
    def render(self) -> str: ...


class FlatPanel(Panel):
    def render(self) -> str:
        return "flat"


class Kit(ABC):
    @abstractmethod
    def button(self) -> Button: ...

    @abstractmethod
    def panel(self) -> Panel: ...


class DarkKit(Kit):
    def button(self) -> Button:
        return FlatButton()

    def panel(self) -> Panel:
        return FlatPanel()


class LightKit(Kit):
    def button(self) -> Button:
        return FlatButton()

    def panel(self) -> Panel:
        return FlatPanel()


class Shape(ABC):
    @abstractmethod
    def draw(self) -> str: ...


class RedCircle(Shape):
    def draw(self) -> str:
        return "rc"


class RedSquare(Shape):
    def draw(self) -> str:
        return "rs"


class BlueCircle(Shape):
    def draw(self) -> str:
        return "bc"


class BlueSquare(Shape):
    def draw(self) -> str:
        return "bs"


class Expression(ABC):
    @abstractmethod
    def eval(self) -> int: ...


class Literal(Expression):
    def eval(self) -> int:
        return 1


class Sum(Expression):
    def eval(self) -> int:
        return 2


class Product(Expression):
    def eval(self) -> int:
        return 3


class Printer:
    def visit_literal(self, node: Literal) -> str:
        return "1"

    def visit_sum(self, node: Sum) -> str:
        return "+"

    def visit_product(self, node: Product) -> str:
        return "*"
