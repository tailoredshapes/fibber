#!/usr/bin/env python3
"""Require every selected unit suite in the shared runner's structured report."""
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
files = pathlib.Path(sys.argv[2]).read_text().splitlines()
expected = {f"{pathlib.Path(f).parent.name}/{pathlib.Path(f).stem}" for f in files}
rows = report["results"]
actual = {row["suite"] for row in rows}
if actual != expected or len(rows) != len(files):
    sys.exit(f"FAIL unit membership: expected {sorted(expected)}, got {sorted(actual)}")
for row in rows:
    if row["status"] != "pass":
        print(f"FAIL {row['id']}: {row['status']} {row.get('message', '')}")
        for step in row["steps"]:
            if step["status"] != "held":
                print(f"  {step['at']}: {step['text']}: expected {step['expected']}, got {step['actual']}")
if any(row["status"] != "pass" for row in rows):
    sys.exit(1)
checks = sum(len(row["steps"]) for row in rows)
print(f"{len(rows)} unit modules, {checks} assertions: all passed")
