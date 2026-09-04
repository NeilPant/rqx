from stripe import charge


def collect_b(amount):
    return charge(amount)
