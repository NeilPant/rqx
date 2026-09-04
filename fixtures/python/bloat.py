def too_many_parameters(a: int, b: int, c: int, d: int, e: int) -> int:
    return a + b + c + d + e


def deeply_nested(rows: list) -> int:
    for row in rows:
        if row > 0:
            while row < 10:
                if row == 5:
                    return row
    return 0


class Overgrown:
    a1 = 1
    a2 = 1
    a3 = 1
    a4 = 1
    a5 = 1
    a6 = 1
    a7 = 1
    a8 = 1
    a9 = 1
    a10 = 1
    a11 = 1
    a12 = 1
    a13 = 1
    a14 = 1
    a15 = 1
    a16 = 1
    a17 = 1
    a18 = 1
    a19 = 1

    def m1(self):
        return 1

    def m2(self):
        return 2

    def m3(self):
        return 3
