# @complexity time=O(n) space=O(1) conf=verified
def index_of(items: list[str], wanted: str) -> int:
    for position in range(len(items)):
        if items[position] == wanted:
            return position
    return -1
