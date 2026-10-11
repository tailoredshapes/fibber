#!/usr/bin/env python3
"""Optional single-run self-build measurements; run on an otherwise idle host."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[4]


def measure(argv, env, directory, label, stdout=subprocess.DEVNULL):
    darwin = platform.system() == "Darwin"
    command = ["/usr/bin/time", "-l" if darwin else "-v", *map(str, argv)]
    started = time.perf_counter()
    p = subprocess.run(command, cwd=directory, env=env, stdout=stdout,
                       stderr=subprocess.PIPE, text=True, timeout=600)
    elapsed = time.perf_counter() - started
    (directory / (label + ".log")).write_text(p.stderr)
    if p.returncode:
        raise RuntimeError(p.stderr)
    pattern = r"(\d+)\s+maximum resident set size" if darwin else r"Maximum resident set size \(kbytes\): (\d+)"
    rss = int(re.search(pattern, p.stderr).group(1))
    stats = re.search(r"liro: hits=(\d+) misses=(\d+) compiled=(\d+)", p.stderr)
    link = re.search(r"time link (\d+) ms", p.stderr)
    return dict(label=label, wall_seconds=round(elapsed, 3),
                peak_rss_kib=rss // 1024 if darwin else rss,
                objects=list(map(int, stats.groups())) if stats else None,
                link_ms=int(link.group(1)) if link else None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fibc", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    fibc = args.fibc.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    directory = args.out.resolve()
    if (directory / "compiler").exists():
        raise RuntimeError("use a fresh output directory")
    shutil.copytree(ROOT / "compiler", directory / "compiler",
                    ignore=shutil.ignore_patterns("tests", "__pycache__"))
    shutil.copytree(ROOT / "lib", directory / "lib")
    llvm = os.environ.get("LLVMLIB", "/opt/homebrew/opt/llvm@21/lib" if platform.system() == "Darwin" else "/usr/lib/llvm-21/lib")
    env = dict(os.environ, FIB_LIB=str(directory / "lib"), FIB_LIRO_CACHE=str(directory / "cache"), FIB_LIRO_MODE="",
               FIB_TIME="1", FIB_LIRO_STATS="1")
    argv = [fibc, "build", "compiler/fibc.fib", "-Icompiler", "-Ilib", "-L", llvm, "-lLLVM-21", "-o"]
    rows = []
    def build(label, extra=()):
        out = directory / label
        row = measure([*argv, out, *extra], env, directory, label)
        row["executable_bytes"] = out.stat().st_size
        rows.append(row)
        print(json.dumps(row), flush=True)
    build("cold")
    build("warm")
    version = directory / "compiler/driver/version.fib"
    original = version.read_text()
    version.write_text(original.replace('"fibc "', '"fibc measured "'))
    build("body-edit")
    assert subprocess.check_output([directory / "body-edit", "--version"], env=env).startswith(b"fibc measured ")
    version.write_text(original)
    timing = directory / "compiler/native/timing.fib"
    original = timing.read_text()
    timing.write_text(original.replace("-> i64 (quot", "-> i32 (trunc i32 (quot").replace("t) 1000000))", "t) 1000000)))"))
    build("abi-edit")
    timing.write_text(original)
    build("whole", ["--whole-program"])
    digests = []
    for mode in ["cold", "whole"]:
        output = directory / (mode + ".lir")
        with output.open("w") as stream:
            row = measure([directory / mode, "emit", "compiler/fibc.fib", "-Icompiler", "-Ilib"],
                          env, directory, "run-" + mode, stdout=stream)
        row["output_sha256"] = hashlib.sha256(output.read_bytes()).hexdigest()
        digests.append(row["output_sha256"])
        rows.append(row)
        print(json.dumps(row), flush=True)
    assert digests[0] == digests[1], "separate and whole compiler outputs differ"
    cpu = subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip() if platform.system() == "Darwin" else re.search(r"^model name\s*:\s*(.*)$", Path("/proc/cpuinfo").read_text(), re.M).group(1)
    report = dict(host=platform.platform(), machine=platform.machine(), cpu=cpu, repetitions=1,
                  compiler=str(fibc), compiler_sha256=hashlib.sha256(fibc.read_bytes()).hexdigest(),
                  command=list(map(str, argv)), rows=rows)
    (directory / "measurements.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
