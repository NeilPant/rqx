def total_length(parts: list[str]) -> int:
    total = 0
    for part in parts:
        total += len(part)
    return total


def shares(parts: list[str], weights: list[float]) -> list[float]:
    out: list[float] = []
    for weight in weights:
        whole = total_length(parts)
        out.append(weight / whole)
    return out


def shares_of_each(weights: list[float]) -> list[float]:
    out: list[float] = []
    for weight in weights:
        own = [str(weight)]
        out.append(weight / total_length(own))
    return out
