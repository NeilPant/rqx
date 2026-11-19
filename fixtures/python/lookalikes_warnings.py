"""Code that resembles a warning and is not one. Every rule should stay silent here."""

import collections
from typing import Generic, TypeVar

# discarded-error: `_T` names a type variable; only `_` throws a value away.
_T = TypeVar("_T")


class Box(Generic[_T]):
    def __init__(self, item: _T):
        self.item = item

    def unwrap(self) -> _T:
        return self.item

# mutable-module-state: nothing writes the table after it is built, so every reader
# sees the same one.
priorities = {"high": 2, "low": 1}


def priority(level):
    return priorities[level]


# working-constructor: handing the message to the parent records it and does no work.
class Refusal(Exception):
    def __init__(self, reason):
        super().__init__(reason)
        self.reason = reason

    def describe(self):
        return f"refused: {self.reason}"


# self-built-collaborators, working-constructor: a counter is where the object keeps
# its data, not a collaborator a test would replace, and making an empty one is not
# work.
class Visits:
    def __init__(self):
        self.seen = collections.Counter()

    def is_first_visit(self, page):
        self.seen[page] += 1
        return self.seen[page] == 1

    # shadows-builtin: a method is reached through the object, so `filter` here
    # hides nothing; the builtin is still what the body calls.
    def filter(self, wanted):
        return list(filter(wanted, self.seen))

    # unused-private-member: Python calls `__len__` for `len(visits)`, and a single
    # underscore is a convention that a subclass or another module may read.
    def __len__(self):
        return len(self.seen)

    def _most_seen(self):
        return self.seen.most_common(1)

    # example-value: documentation shows an example; it is never a value that ships.
    def visited_hosts(self):
        """The hosts of the pages seen, such as example.com for https://example.com/a."""
        return {page.split("/")[2] for page in self.seen}


# repeated-computation-in-loop: what a for loop walks is computed once, before the
# first pass, and a call in a return runs at most once however many passes come first.
def _sizes_of(parts):
    return [len(part) for part in parts]


def _total_size(parts):
    return sum(len(part) for part in parts)


def first_oversized(parts, limit):
    for size in _sizes_of(parts):
        if size > limit:
            return _total_size(parts)
    return 0
