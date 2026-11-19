"""Code that resembles a warning and is not one. Every rule should stay silent here."""


# working-constructor: handing the message to the parent records it and does no work.
class Refusal(Exception):
    def __init__(self, reason):
        super().__init__(reason)
        self.reason = reason

    def describe(self):
        return f"refused: {self.reason}"
