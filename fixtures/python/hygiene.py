import sys
from .cycle_b import *
from .cycle_b import spin
from .cycle_c import wobble
from .cycle_a import orbit
from ....shared.far import far
from .cycle_a import orbit as orbit_again


def pricing(cart: list, apply_discount: bool) -> int:
    never_read = len(cart)
    total = sum(cart)

    # legacy = total * 0.2
    # return legacy + shipping

    if not apply_discount and total > 4999 and len(cart) > 12 and orbit():
        return total
    else:
        total = total * 4999
    try:
        return total + 4999
    except ValueError:
        pass
    return 0


def unreachable(value: int) -> int:
    # rqx-ignore magic-number
    seed = 1
    return value + seed
    value = 1


# type: ignore
suppressed = 1


def ship(order) -> str:
    return order.customer.address.city.strip().upper()

# ---------------- helpers ----------------
# Here's the helper you asked for! 🎉
# Sorry, this one is ugly.

def nothing_happens() -> None:
    pass


def forward_orbit(seed):
    return orbit(seed)


def still_to_write():
    raise NotImplementedError("not implemented")


def reraises():
    try:
        return orbit(1)
    except ValueError as error:
        raise error


def traced():
    print("traced", spin)


def maybe_count(rows):
    if len(rows) == 0:
        return
    return len(rows)


def closes_out(rows):
    try:
        return len(rows)
    finally:
        return 0


def catches_everything(rows):
    try:
        return len(rows)
    except Exception:
        return 0


def refuses():
    raise Exception("no")


def stops(code):
    sys.exit(code)


def requests(url):
    return _fetch_page(url, True)


def _fetch_page(url, retry):
    return url if retry else url


def narrated(rows):
    # set the total
    total = 0
    # walk the rows
    for row in rows:
        total += row
    # halve it
    total = total / 2
    # give it back
    return total


# Works out the score for the given user id, and gives back the score it
# worked out, having looked at the id that was handed in by whoever called
# it, which is the id of the user whose score is wanted here. The id is not
# checked, because whoever calls this has already checked it, and checking
# twice would be slower for no benefit anybody has ever measured. If the id
# is wrong the answer is wrong, which is the same as it has always been in
# every version of this that anybody has written since the beginning.
def score_of(user_id):
    return user_id


def guarded_add(a, b, c):
    if a < 0:
        return 0
    if b < 0:
        return 0
    if c < 0:
        return 0
    return a + b + c


def netted(rows):
    try:
        total = 0
        for row in rows:
            total += row
        return total
    except ValueError:
        return 0


def score_of_user(user_id):
    """Score of user id"""
    return user_id
