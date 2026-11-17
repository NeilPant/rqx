import functools

_page_sizes: dict[str, int] = {}
_recent_sizes: dict[str, int] = {}


def remember_page(key: str, size: int) -> int:
    _page_sizes[key] = size
    return size


def remember_recent_page(key: str, size: int) -> int:
    _recent_sizes[key] = size
    if len(_recent_sizes) > 100:
        _recent_sizes.clear()
    return size


@functools.lru_cache(maxsize=None)
def square_of(number: int) -> int:
    return number * number


@functools.lru_cache(maxsize=128)
def cube_of(number: int) -> int:
    return number * number * number
