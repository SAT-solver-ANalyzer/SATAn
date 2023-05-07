#! /usr/bin/env python3
# Script for extracting metrics from cadical output
# This is written to be moderately fast while staying very readable
# TODO: Add handling for invalid input files

import sys
import re

clauses_regex = re.compile(r"(?P<clauses>[0-9]+)[^0-9]+(?P<parse_time>[0-9]+\.[0-9]+)")
memory_regex = re.compile(r"(?P<memory>[0-9]+\.[0-9]+)")
float_metric_regex = re.compile(r"(?P<metrics>[0-9]+\.[0-9]+)")
simple_metric_regex = re.compile(r"c [a-z]+: +(?P<metrics>[0-9]+)")

def extract_float_metric(line: str) -> float:
    match = float_metric_regex.search(line)
    assert match is not None

    return float(match["metrics"])

def extract_int_metric(line: str) -> int:
    match = simple_metric_regex.search(line)
    assert match is not None

    return int(match["metrics"])


if __name__ == "__main__":
    metrics = {"restarts": 0, "conflicts": 0, "conflict_literals": 0} 

    for line in sys.stdin.readlines():
        if line.startswith("c parsed "):
            match = clauses_regex.search(line)
            assert match is not None
            metrics["parse_time"] = int(float(match["parse_time"]) * 1000)
            metrics["number_of_clauses"] = int(match["clauses"])
        elif line.startswith("c *") and "number_of_variables" not in metrics:
            matches = re.findall(r"[0-9.]+", line)
            metrics["number_of_variables"] = int(matches[-2])
        elif line.startswith("s UNSATISFIABLE"):
            metrics["satisfiable"] = -1
        elif line.startswith("s SATISFIABLE"):
            metrics["satisfiable"] = 1
        elif line.startswith("c propagations:"):
            metrics["propagations"] = extract_int_metric(line)
        elif line.startswith("c restarts:"):
            metrics["restarts"] = extract_int_metric(line)
        elif line.startswith("c conflicts:"):
            metrics["conflicts"] = extract_int_metric(line)
        elif line.startswith("c maximum resident set size"):
            metrics["memory_usage"] = int(extract_float_metric(line) * 1e6)
        elif line.startswith("c total process time"):
            metrics["runtime"] = int(extract_float_metric(line) * 1000)

    print("---")
    for metric, value in metrics.items():
        print(f"{metric}: {value}")

    sys.exit(0)
