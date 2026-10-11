#!/usr/bin/env python3
"""Generate shared unit runners. Test modules, not source rewriting, supply suites."""
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
TESTS = ROOT / "compiler/tests"


def names(file):
    return [line.split()[0] for line in (TESTS / file).read_text().splitlines()
            if line.strip() and not line.lstrip().startswith("#")]


def generate(out, selected):
    runnable = names("units.run")
    listed = runnable + names("units.pending") + names("units.elsewhere")
    present = {str(p.relative_to(ROOT)) for p in TESTS.glob("*/unit-*.fib")}
    if len(listed) != len(set(listed)) or set(listed) != present:
        raise ValueError("unit inventory differs: " + str(sorted(set(listed) ^ present)))
    if len(selected) != len(set(selected)) or not set(selected) <= set(runnable):
        raise ValueError("select distinct files from units.run")
    groups = {}
    for file in selected:
        part = pathlib.Path(file).parent.name
        group = part if part in {"emit", "own", "types"} else "rest"
        groups.setdefault(group, []).append(file)
    out.mkdir(parents=True, exist_ok=True)
    for group, files in groups.items():
        modules = [".".join(pathlib.Path(f).with_suffix("").parts[1:]) for f in files]
        requires = " ".join(f"[{module} :as u{i}]" for i, module in enumerate(modules))
        suites = " ".join(f"(u{i}/tests)" for i in range(len(files)))
        source = (f"(ns main (:require {requires}) (:use fib.test.run))\n"
                  f"(defun main () -> i64\n"
                  f"  (run-main-with [{suites}]\n"
                  '    (into ["--isolate" "fork" "--timeout" "300"] (vec (args)))))\n')
        (out / (group + ".fib")).write_text(source)
        (out / (group + ".files")).write_text("\n".join(files) + "\n")
        print(group)


if __name__ == "__main__":
    try:
        generate(pathlib.Path(sys.argv[1]), sys.argv[2:])
    except (ValueError, OSError) as error:
        sys.exit("unit groups: " + str(error))
