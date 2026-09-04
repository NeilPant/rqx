from stripe import charge


def collect_c(amount):
    return charge(amount)
