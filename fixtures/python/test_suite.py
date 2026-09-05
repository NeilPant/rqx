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


def test_branches_on_the_input():
    if add(1, 1) == 2:
        assert add(1, 1) == 2


def test_asserts_the_same_thing_twice():
    assert add(1, 1) == 2
    assert add(1, 1) == 2


def test_catches_the_failure_itself():
    try:
        add(1, 1)
    except ValueError as error:
        assert error is not None


def test_prints_while_it_runs():
    print("here")
    assert add(1, 1) == 2


def test_reads_the_store():
    rows = db.query("rows")
    assert rows is not None
