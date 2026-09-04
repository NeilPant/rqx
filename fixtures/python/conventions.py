retry_limit = 3
retryBackoff = 250


def query(user_id: str) -> str:
    return f"SELECT id, name FROM users WHERE id = '{user_id}'"


def window(items: list, cursor: int) -> list:
    head = items[cursor + 1]
    tail = items[cursor + 1]
    spare = items[cursor + 1]
    return [value for value in [head, tail, spare] if value]


limits = {"retry_limit": retry_limit, "retryBackoff": retryBackoff}
