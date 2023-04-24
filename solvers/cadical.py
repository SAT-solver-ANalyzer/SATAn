#!/usr/bin/env python3
# Script for extracting metrics from cadical output
# This is written to be moderately fast while staying very readable
# TODO: Finish this

import sys
import re

clauses_regex = re.compile(
    r"(?P<clauses>[0-9]+)[^0-9]+(?P<parse_time>[0-9]+\.[0-9]+)"
)

if __name__ == "__main__":
    metrics = {}

    for line in sys.stdin.readlines():
        if line.startswith("c parsed "):
            match = clauses_regex.match(line)
            assert match is not None
            metrics["parse_time"] = int(float(match["parse_time"]) * 1000)
            metrics["clauses"] = int(match["parse_time"])
        elif line == "s UNSATISFIABLE":
            metrics["satisfibility"] = -1
        elif line == "s SATISFIABLE":
            metrics["satisfibility"] = 1
        elif line.startswith("")

    for metric, value in metrics.items():
        print("{metric}: {value}")

    sys.exit(0)
