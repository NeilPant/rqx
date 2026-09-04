from stripe import charge


def collect_a(amount):
    return charge(amount)
