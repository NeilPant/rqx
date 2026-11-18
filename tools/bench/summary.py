"""Prints the summary table of tools/bench/run.sh from the JSON hyperfine exported.

    python3 -I summary.py OUT_DIR [yes]

OUT_DIR holds cold.json, warm.json and warm-after-edit.json. A second argument of
`yes` means each file holds a baseline result followed by the current one.

hyperfine has changed where a run's time is written, so the mean is looked up in each
known place and, when none holds one, computed from the individual run times:

  1.x   result["mean"], and result["times"], a list of seconds
  2.x   result["summary"]["time_wall_clock"]["mean"], and
        result["measurements"][i]["time_wall_clock"]["value"]

Memory figures and hardware counters sit beside the times in 2.x and are never read as
a time.
"""

import json
import os
import sys

SCENARIOS = ("cold", "warm", "warm-after-edit")
TIME_KEYS = ("time_wall_clock", "wall_time", "time")


def is_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def recorded_mean(result):
    for where in (result, result.get("summary")):
        if not isinstance(where, dict):
            continue
        if is_number(where.get("mean")):
            return where["mean"]
        for key in TIME_KEYS:
            nested = where.get(key)
            if isinstance(nested, dict) and is_number(nested.get("mean")):
                return nested["mean"]
    return None


def seconds_of(measurement):
    if is_number(measurement):
        return measurement
    if isinstance(measurement, dict):
        for key in TIME_KEYS:
            nested = measurement.get(key)
            if is_number(nested):
                return nested
            if isinstance(nested, dict) and is_number(nested.get("value")):
                return nested["value"]
    return None


def run_times(result):
    raw = result.get("times")
    if not isinstance(raw, list):
        raw = result.get("measurements")
    if not isinstance(raw, list):
        return []
    times = [seconds_of(item) for item in raw]
    return [time for time in times if time is not None]


def mean(result):
    recorded = recorded_mean(result)
    if recorded is not None:
        return recorded
    times = run_times(result)
    if times:
        return sum(times) / len(times)
    raise ValueError(f"no mean and no run times in hyperfine's result; keys: {sorted(result)}")


def render(out, paired):
    lines = ["summary (mean seconds; change is current against baseline)"]
    for scenario in SCENARIOS:
        with open(os.path.join(out, scenario + ".json")) as f:
            results = json.load(f)["results"]
        current = mean(results[-1])
        if paired:
            base = mean(results[0])
            change = (current - base) / base * 100
            lines.append(
                f"  {scenario:<17} baseline {base:8.3f}   current {current:8.3f}   {change:+6.1f}%"
            )
        else:
            lines.append(f"  {scenario:<17} current {current:8.3f}")
    return "\n".join(lines)


def main(argv):
    out = argv[1]
    paired = len(argv) > 2 and argv[2] == "yes"
    try:
        print(render(out, paired))
    except ValueError as error:
        sys.exit(f"run.sh: {error}")


if __name__ == "__main__":
    main(sys.argv)
