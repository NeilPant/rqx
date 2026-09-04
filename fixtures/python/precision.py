from typing import Any

# @author someone
# Created on 2019-04-02


def widen(raw: Any) -> str:
    text: Any = raw
    return text.strip()


# user name
user_name = "ada"


class Loose:
    def __init__(self, payload: Any):
        self.payload = payload


def stamp() -> str:
    return "ok"
