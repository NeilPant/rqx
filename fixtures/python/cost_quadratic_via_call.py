def index_of_item(items: list[str], wanted: str) -> int:
    for position in range(len(items)):
        if items[position] == wanted:
            return position
    return -1


def first_occurrences(items: list[str]) -> list[str]:
    firsts: list[str] = []
    for at in range(len(items)):
        if index_of_item(items, items[at]) == at:
            firsts.append(items[at])
    return firsts


def describe(text: str) -> str:
    return text.strip().upper()


def describe_each(items: list[str]) -> list[str]:
    out: list[str] = []
    for item in items:
        out.append(describe(item))
    return out
