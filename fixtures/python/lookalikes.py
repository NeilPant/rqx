# Code that resembles a finding and is not one. Every rule should stay silent here.


# output-parameter: a class-level annotation declares a field, even when a method
# takes a parameter of the same name.
class Envelope:
    sender: str

    def __init__(self, sender: str) -> None:
        self.sender = sender

    def signed(self, body: str) -> str:
        return body + self.sender


# output-parameter: `factor` in `counted` is a local that shares its name with
# this parameter, not this parameter.
def scaled(factor: int) -> int:
    return factor * 2


def counted(items: list[str]) -> int:
    factor = len(items)
    factor += 1
    return factor
