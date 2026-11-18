import unittest

import pytest

from .clean import add


class TestChecks(unittest.TestCase):
    def test_compares_with_unittest(self):
        self.assertEqual(add(1, 1), 2)

    def test_expects_a_raise(self):
        with self.assertRaises(TypeError):
            add(None, 1)

    def test_fails_by_hand(self):
        if add(1, 0) != 1:
            self.fail("add(1, 0) is not 1")

    def test_checks_through_a_helper(self):
        self.sum_is(add(1, 1), 2)

    def test_runs_a_table(self):
        for left, right, want in [(0, 0, 0), (1, 0, 1)]:
            with self.subTest(left=left):
                self.assertEqual(add(left, right), want)

    def sum_is(self, got, want):
        self.assertEqual(got, want)


def test_raises_with_pytest():
    with pytest.raises(TypeError):
        add(None, 1)


def test_skips_without_the_feature():
    if not hasattr(add, "__call__"):
        pytest.skip("add is not callable")
    assert add(0, 0) == 0


def test_runs_a_named_table():
    rows = {"zero": (0, 0), "one": (1, 1)}
    for name, (value, want) in rows.items():
        assert add(value, 0) == want, name


@pytest.mark.parametrize("value", [0, 1])
def test_runs_with_parameters(value):
    assert add(value, 0) == value


def test_decides_three_ways():
    total = add(0, 1)
    if total > 1:
        total = add(total, -1)
    else:
        total = add(total, 1)
    while total < 2:
        total = add(total, 1)
    if total == 2:
        total = add(total, 0)
    assert total == 2
