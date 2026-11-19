# Code that resembles a finding and is not one. Every rule should stay silent here.
import copy

from .lookalikes import ranked


# self-asserting-test: each side runs the code, so the check holds it to answering
# alike on every call.
def test_ranking_is_stable():
    assert ranked([1]) == copy.copy(ranked([1]))


# unused-parameter: pytest sets up every fixture a test names, so one the body never
# reads is still there for its setup.
def test_ranking_runs_in_a_clean_directory(tmp_path):
    assert ranked([]) == []
