from leaf import total


# @complexity time=O(n^2) space=O(1) conf=verified
def shares(values: list[int]) -> float:
    largest = 0.0
    for value in values:
        largest = max(largest, value / total(values))
    return largest
