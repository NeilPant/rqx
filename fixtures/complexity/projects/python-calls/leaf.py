# Two modules, the second calling into the first.


# @complexity time=O(n) space=O(1) conf=verified
def total(values: list[int]) -> int:
    result = 0
    for value in values:
        result += value
    return result
