#! /usr/bin/env python3
# Script for extracting metrics from minisat output
# This is written to be moderately fast while staying very readable
# TODO: Add handling for invalid input files

import sys
import re

float_metric_regex = re.compile(r"(?P<metrics>[0-9]+\.[0-9]+)")
simple_metric_regex = re.compile(r"(?P<metrics>[0-9]+)")


def extract_float_metric(line: str) -> float:
    match = float_metric_regex.search(line)
    assert match is not None

    return float(match["metrics"])


def extract_int_metric(line: str) -> int:
    match = simple_metric_regex.search(line)
    assert match is not None

    return int(match["metrics"])


if __name__ == "__main__":
    metrics = {}

    for line in sys.stdin.readlines():
        if line.startswith("|  Number of variables:"):
            metrics["number_of_variables"] = extract_int_metric(line)
        elif line.startswith("|  Number of clauses:"):
            metrics["number_of_clauses"] = extract_int_metric(line)
        elif line.startswith("|  Parse time:"):
            metrics["parse_time"] = int(extract_float_metric(line) * 1000)
        elif line.startswith("restarts"):
            metrics["restarts"] = extract_int_metric(line)
        elif line.startswith("conflicts"):
            metrics["conflicts"] = extract_int_metric(line)
        elif line.startswith("propagations"):
            metrics["propagations"] = extract_int_metric(line)
        elif line.startswith("conflict literals"):
            metrics["conflict_literals"] = extract_int_metric(line)
        elif line.startswith("Memory used"):
            metrics["memory_usage"] = int(extract_float_metric(line) * 1e6)
        elif line.startswith("CPU time"):
            metrics["runtime"] = int(extract_float_metric(line) * 1000)
        elif line.startswith("UNSATISFIABLE"):
            metrics["satisfiable"] = -1
        elif line.startswith("SATISFIABLE"):
            metrics["satisfiable"] = 1

    print("---")
    for metric, value in metrics.items():
        print(f"{metric}: {value}")

    sys.exit(0)
