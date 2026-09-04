import time

import pytest

from .clean import add


@pytest.mark.skip(reason="pricing is being rewritten")
def test_discounts():
    assert add(1, 2) == 3


def test_ships():
    time.sleep(500)
    assert add(1, 1) == 2


def test_checks_nothing():
    value = add(1, 1)
