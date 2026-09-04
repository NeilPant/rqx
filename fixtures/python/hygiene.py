from .cycle_a import orbit


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
    return value
    value = 1


# type: ignore
suppressed = 1


def ship(order) -> str:
    return order.customer.address.city.strip().upper()
