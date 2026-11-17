def owners_of(orders, users):
    names = []
    for order in orders:
        for user in users:
            if order.user_id == user.id:
                names.append(user.name)
    return names


def same_order(orders):
    pairs = 0
    for left in orders:
        for right in orders:
            if left.id == right.user_id:
                pairs += 1
    return pairs
