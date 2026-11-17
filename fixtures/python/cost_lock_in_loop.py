import threading


def tally_under_one_lock(lock: threading.Lock, rows: list[int]) -> int:
    total = 0
    for row in rows:
        lock.acquire()
        total += row
        lock.release()
    return total


def tally_in_a_with_block(lock: threading.Lock, rows: list[int]) -> int:
    total = 0
    for row in rows:
        with lock:
            total += row
    return total


def tally_under_each_lock(locks: list[threading.Lock], rows: list[int]) -> int:
    total = 0
    for at in range(len(rows)):
        locks[at].acquire()
        total += rows[at]
        locks[at].release()
    return total
