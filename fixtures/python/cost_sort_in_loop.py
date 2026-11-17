def rank_each(groups: list[list[int]], base: list[int]) -> list[list[int]]:
    ranked: list[list[int]] = []
    for group in groups:
        base.sort()
        ranked.append(group + base)
    return ranked


def rank_own(groups: list[list[int]]) -> list[list[int]]:
    ranked: list[list[int]] = []
    for group in groups:
        group.sort()
        ranked.append(group)
    return ranked
