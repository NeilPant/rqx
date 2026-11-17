from leaf import index_of


# @complexity time=O(n*m) space=O(n) conf=verified rule=quadratic-via-call
def first_positions(items: list[str], wanted: list[str]) -> list[int]:
    found: list[int] = []
    for name in wanted:
        found.append(index_of(items, name))
    return found
