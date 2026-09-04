def endpoint(region: str, secure: bool, retries: int) -> str:
    if len(region) > 0:
        if secure:
            if retries > 0:
                return "https://api.example.com/v2/orders"
    return ""


def score(a: int, b: int, c: int, d: int) -> int:
    return (a + b) * (c - d) + max(a, b) + min(c, d) - (a * d)
