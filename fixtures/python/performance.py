import re
import time


def keep_wanted(rows, ids):
    wanted = list(ids)
    kept = []
    for row in rows:
        if row in wanted:
            kept.append(row)
    return kept


def sum_cube(grid):
    total = 0
    for plane in grid:
        for line in plane:
            for cell in line:
                total += cell
    return total


def query_each(client, ids):
    rows = []
    for item in ids:
        rows.append(client.get(item))
    return rows


async def read_each(reader, paths):
    texts = []
    for path in paths:
        texts.append(await reader.read(path))
    return texts


def count_digits(lines):
    hits = 0
    for line in lines:
        digits = re.compile("^[0-9]+$")
        hits += 1 if digits.match(line) else 0
    return hits


def join_by_copy(parts):
    text = ""
    for part in parts:
        text = text + part
    return text


def fibonacci(step):
    if step < 2:
        return step
    return fibonacci(step - 1) + fibonacci(step - 2)


async def settings_of(path):
    time.sleep(1)
    return path


def schedules(rows):
    out = []
    for at in range(len(rows)):
        def later():
            return at
        out.append(later)
    return out


def prunes(rows):
    for row in rows:
        if row < 0:
            rows.remove(row)
    return len(rows)


async def both_waits():
    first = await _load_one()
    second = await _load_two()
    return first + second


def guards(value):
    assert value > 0
    return value


async def _load_one():
    return 1


async def _load_two():
    return 2
