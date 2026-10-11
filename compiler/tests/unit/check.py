#!/usr/bin/env python3
"""Exercise the public unit API and runner with passing and deliberately broken tests."""
import json
import pathlib
import subprocess
import sys
import tempfile

exe = sys.argv[1]


def run(code, *args):
    result = subprocess.run([exe, *args], capture_output=True, text=True, timeout=20)
    assert result.returncode == code, (args, result.returncode, result.stdout, result.stderr)
    return result.stdout


def report(code, *args):
    return json.loads(run(code, "--format", "json", *args))


for isolation in ["none", "task", "fork"]:
    good = report(0, "--only", "passing", "--isolate", isolation)
    assert good["summary"]["pass"] == 1
    assert len(good["results"][0]["steps"]) == 7
bad = report(1, "--only", "failing", "--isolate", "fork")
steps = bad["results"][0]["steps"]
assert [s["status"] for s in steps] == ["broke", "broke", "held"]
assert (steps[1]["expected"], steps[1]["actual"]) == ("1", "2")
assert all("fixture.fib:" in s["at"] for s in steps)
empty = report(1, "--only", "empty")
assert "no Then step" in empty["results"][0]["message"]
for isolation in ["task", "fork"]:
    trapped = report(1, "--only", "trap", "--isolate", isolation)
    assert trapped["summary"]["trap"] == 1 and trapped["summary"]["pass"] == 1
hung = report(1, "--only", "hang", "--timeout", "1")
assert hung["summary"]["timeout"] == 1 and hung["summary"]["pass"] == 1
assert "not ok 1" in run(1, "--only", "failing", "--format", "tap")
assert "planted equality failure" in run(1, "--only", "failing")
assert "hanging" in run(0, "--list")
run(2, "--only", "does-not-exist")
run(2, "--duplicate")
print("unit API: assertions, values, source positions, fresh logs, isolation, timeout and reporters verified")

# A successful executable must still account for every selected module. Plant omissions,
# duplicates and failed rows in reports, and reject invalid runner selections.
tests = pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as scratch:
    out = pathlib.Path(scratch)
    selection = "compiler/tests/types/unit-args.fib"
    files = out / "types.files"
    files.write_text(selection + "\n")
    row = {"suite": "types/unit-args", "id": "types-unit-args/checks",
           "status": "pass", "steps": []}
    for rows, code in [([row], 0), ([], 1), ([row, row], 1),
                       ([dict(row, suite="wrong")], 1),
                       ([dict(row, status="fail", message="planted")], 1)]:
        path = out / "report.json"
        path.write_text(json.dumps({"results": rows}))
        result = subprocess.run([sys.executable, str(tests / "unit-report.py"),
                                 str(path), str(files)], capture_output=True, text=True)
        assert result.returncode == code, (rows, result.stdout, result.stderr)
    for selected, code in [([selection], 0), ([selection, selection], 1),
                           (["compiler/tests/types/unit-builtins.fib"], 1)]:
        result = subprocess.run([sys.executable, str(tests / "unit-groups.py"),
                                 str(out), *selected], capture_output=True, text=True)
        assert result.returncode == code, (selected, result.stdout, result.stderr)
print("unit groups: omissions, duplicates, failed reports and invalid selections rejected")
