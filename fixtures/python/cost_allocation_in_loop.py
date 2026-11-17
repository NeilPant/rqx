def describe_rows(rows: list[int]) -> int:
    total = 0
    for row in rows:
        pair = [row, row + 1]
        label = {"row": row}
        spare = [row]
        total += len(pair) + len(spare) + len(label)
    return total


def buffer_each_row(rows: list[int]) -> int:
    total = 0
    for row in rows:
        buffer = bytearray(65536)
        total += len(buffer) + row
    return total


def pair_each_row(rows: list[int]) -> int:
    total = 0
    for row in rows:
        pair = [row, row + 1]
        total += len(pair)
    return total
