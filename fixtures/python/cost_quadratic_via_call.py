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


def missing_from(items: list[str], known: list[str]) -> list[str]:
    missing: list[str] = []
    for item in items:
        if index_of_item(known, item) < 0:
            missing.append(item)
    return missing
