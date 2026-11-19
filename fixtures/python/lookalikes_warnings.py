"""Code that resembles a warning and is not one. Every rule should stay silent here."""

import collections

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

    # example-value: documentation shows an example; it is never a value that ships.
    def visited_hosts(self):
        """The hosts of the pages seen, such as example.com for https://example.com/a."""
        return {page.split("/")[2] for page in self.seen}
