"""Checks summary.py against each shape of result hyperfine has written.

    python3 -I tools/bench/test_summary.py
"""

import json
import os
import sys
import tempfile
import unittest

# Importing summary.py would otherwise leave a __pycache__ directory in the repository.
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import summary  # noqa: E402


def wall(value):
    return {"time_wall_clock": {"value": value}, "memory_peak_resident": {"value": 9e6}}


class MeanTest(unittest.TestCase):
    def test_hyperfine_1_has_the_mean_on_the_result(self):
        self.assertEqual(summary.mean({"mean": 1.5, "times": [9.0]}), 1.5)

    def test_hyperfine_1_without_a_mean_averages_times(self):
        self.assertAlmostEqual(summary.mean({"times": [1.0, 2.0, 3.0]}), 2.0)

    def test_hyperfine_2_has_the_mean_in_the_wall_clock_summary(self):
        result = {
            "summary": {
                "time_wall_clock": {"mean": 0.75},
                "memory_peak_resident": {"mean": 123456789.0},
            },
            "measurements": [wall(5.0)],
        }
        self.assertEqual(summary.mean(result), 0.75)

    def test_hyperfine_2_without_a_summary_mean_averages_the_wall_clock_values(self):
        result = {"summary": {}, "measurements": [wall(1.0), wall(3.0)]}
        self.assertAlmostEqual(summary.mean(result), 2.0)

    def test_measurements_as_plain_numbers(self):
        self.assertAlmostEqual(summary.mean({"measurements": [2.0, 4.0]}), 3.0)

    def test_measurements_with_a_bare_wall_time_number(self):
        result = {"measurements": [{"time_wall_clock": 2.0}, {"time_wall_clock": 6.0}]}
        self.assertAlmostEqual(summary.mean(result), 4.0)

    def test_memory_is_never_read_as_a_time(self):
        result = {"measurements": [{"memory_peak_resident": {"value": 9e6}}]}
        with self.assertRaises(ValueError):
            summary.mean(result)

    def test_a_result_with_nothing_to_average_names_its_keys(self):
        with self.assertRaisesRegex(ValueError, "keys: \\['command', 'name'\\]"):
            summary.mean({"command": "x", "name": "y"})


class RenderTest(unittest.TestCase):
    def render(self, results_per_scenario, paired):
        with tempfile.TemporaryDirectory() as out:
            for scenario, results in results_per_scenario.items():
                with open(os.path.join(out, scenario + ".json"), "w") as f:
                    json.dump({"results": results}, f)
            return summary.render(out, paired)

    def test_a_baseline_run_reports_the_change(self):
        old = {"mean": 2.0}
        new = {"summary": {"time_wall_clock": {"mean": 1.0}}}
        text = self.render({name: [old, new] for name in summary.SCENARIOS}, True)
        self.assertIn("baseline    2.000   current    1.000", text)
        self.assertIn("-50.0%", text)

    def test_a_single_run_reports_the_current_mean(self):
        result = {"measurements": [wall(1.0), wall(2.0)]}
        text = self.render({name: [result] for name in summary.SCENARIOS}, False)
        self.assertEqual(text.splitlines()[0].split(";")[0], "summary (mean seconds")
        self.assertEqual(text.count("current    1.500"), 3)


if __name__ == "__main__":
    unittest.main()
