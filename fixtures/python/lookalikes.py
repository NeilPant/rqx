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


# empty-catch: an except clause that says why it does nothing has handled the error.
def parsed_or_nothing(text):
    try:
        return int(text)
    except ValueError:
        # Malformed input means no setting, which the caller already handles.
        pass
    return None


# unreachable-code: the `yield` after the return makes this a generator that yields
# nothing, which is how such a generator is written.
def quiet_events():
    return
    yield


# shadowed-handler: each handler takes something the ones above it do not.
def first_line(path):
    try:
        with open(path) as handle:
            return handle.readline()
    except FileNotFoundError:
        return ""
    except OSError:
        return None
    except UnicodeDecodeError:
        return "?"


# self-comparison: only NaN differs from itself, so this is how NaN is detected.
def is_missing(reading: float) -> bool:
    return reading != reading


# exit-from-finally: the break leaves a loop inside the clause, and Python keeps the
# failure in flight as the context of the one raised here.
def drained(pending: list) -> int:
    try:
        return len(pending)
    finally:
        while pending:
            if pending[-1] is None:
                break
            pending.pop()
        if pending:
            raise RuntimeError("left unfinished")
