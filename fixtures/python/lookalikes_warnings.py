"""Code that resembles a warning and is not one. Every rule should stay silent here."""

import collections


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
