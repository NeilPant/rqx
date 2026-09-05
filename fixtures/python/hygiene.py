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
