# Code that resembles a finding and is not one. Every rule should stay silent here.
import atexit
import functools
import re
import time

# distant-declaration, short-name-wide-scope: `TZ` belongs to the module, so being
# read at the bottom of the file is where it belongs.
TZ = "UTC"


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


# ambient-dependency: the clock is a parameter, and the ambient one is only its
# default.
def elapsed_since(start: float, clock=time.time) -> float:
    return clock() - start


# mixed-language-file: an error message that names an update is not SQL.
def update_failure(user_id: str) -> str:
    return "Failed to update user " + user_id


def zone_label() -> str:
    return "zone " + TZ


# magic-number: each 30 below is the value of a declaration, which names it.
def timeout_budget() -> int:
    connect_timeout_seconds = 30
    read_timeout_seconds = 30
    idle_timeout_seconds = (30)
    return connect_timeout_seconds + read_timeout_seconds + idle_timeout_seconds


# import-time-work: registering a handler runs nothing until the event arrives.
def _flush() -> None:
    print("flushed")


atexit.register(_flush)
atexit.register(lambda: print("closed"))



# no-base-case: the method calls the function it is named after, not itself.
def render(text: str) -> str:
    return text.strip()


class Page:
    def render(self, text: str) -> str:
        return render(text) + "\n"



# closure-over-loop-variable: `re.sub` calls the function before it returns.
def suffixed(text: str, suffixes: list[str]) -> list[str]:
    out = []
    for suffix in suffixes:
        out.append(re.sub("o", lambda match: match.group(0) + suffix, text))
    return out



# imprecise-type: `Any` imported or named outside an annotation is not a type.
def first_truthy(values: list[int]) -> int:
    Any = max(values, default=0)
    return Any * 2 if Any else 0



# distant-declaration: the table is read while it is declared, and used on the next line.
def spelled(word: str) -> int:
    code_words = [
        "alfa",
        "bravo",
        "charlie",
        "delta",
        "echo",
        "foxtrot",
        "golf",
        "hotel",
        "india",
        "juliett",
        "kilo",
        "lima",
        "mike",
        "november",
        "oscar",
        "papa",
        "quebec",
        "romeo",
        "sierra",
        "tango",
        "uniform",
        "victor",
        "whiskey",
        "xray",
        "yankee",
        "zulu",
    ]
    return code_words.index(word)



# unused-parameter: signatures fixed by the language, a framework or a caller.
class Router:
    def __init__(self) -> None:
        self.routes: dict = {}

    def route(self, path: str):
        return functools.partial(self.register, path)

    def register(self, path: str, handler):
        self.routes[path] = handler
        return handler


ROUTER = Router()


@ROUTER.route("/health")
def health(request):
    return "ok"


class Session:
    @classmethod
    def opened(cls, *args, **kwargs):
        return cls()

    def close(self, _reason):
        return self


def ranked(rows):
    return sorted(rows, key=lambda row: 0)
