#!/usr/bin/env python3
"""compiler/tests/wasm/suite.py: a case directory against the wasm32-wasi build (docs/design/wasm.md 4).

  suite.py --fibc F DIR [--only PREFIX..] [-j N] [--expected FILE] [--no-simd] [--runtime node|wasmtime] [--results OUT.tsv] [--scratch DIR]

Each case of DIR (the `*.fib` files, and the `main.fib` of each subdirectory that has one), as `F cases` finds them, is judged by the verdict its header fixes
(spec/method.md rule 3), run as a wasm module instead of a native program:
  accept  built with `F build --target wasm32-wasi`, run, the last line of standard output is `result` and the status 0;
  trap    the status is not 0 and standard error holds `trap: MESSAGE`, the message of the header;
  reject  `F build` refuses the program (the checker does not depend on the target, so this is a short check, not a second run of it).
The audit (`audit`, `allocs`, `leaks`) reads the free trace of a native run and is not checked here. A case is one of:
  PASS    the verdict holds;
  FAIL    it does not (the reason is printed);
  OPEN    a case whose header has `open:` (spec/method.md: an item the spec has not settled) that does not pass: excused, as the native suite excuses it.
`--expected FILE` lists the cases that are known not to pass, one `DIR/CASE STATUS REASON` per line, as scripts/ci-stage2.expected does: the run fails on a case that is
not there and does not pass, and on a case that is there and passes. The exit status is 0 when the run matches the list, 1 otherwise, 2 for a usage error.
"""
import argparse, os, re, resource, subprocess, sys, tempfile, shutil, time
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
RUN_MJS = os.path.join(HERE, "run.mjs")


def parse_header(path):
    """The `;; key: value` lines at the top of a case file."""
    fields = {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            m = re.match(r"^;;\s*([A-Za-z0-9_-]+):\s*(.*?)\s*$", line)
            if not m:
                break
            fields.setdefault(m.group(1), m.group(2))
    return fields


def list_cases(directory):
    """Rows (key, path) in file-name order: `*.fib`, and `main.fib` of a subdirectory."""
    rows = []
    for name in sorted(os.listdir(directory)):
        p = os.path.join(directory, name)
        if os.path.isdir(p):
            if os.path.exists(os.path.join(p, "main.fib")):
                rows.append((name, os.path.join(p, "main.fib")))
        elif name.endswith(".fib"):
            rows.append((name, p))
    return rows


class Env:
    def __init__(self, a):
        self.fibc = a.fibc
        self.scratch = a.scratch
        self.runtime = a.runtime
        self.simd = not a.no_simd
        self.extra = a.extra or []
        self.timeout = a.timeout


def big_stack():
    """In the child: a 1 GB stack for node, whose own stack the wasm frames of a deep recursion use (--stack-size is below it)."""
    try:
        soft, hard = resource.getrlimit(resource.RLIMIT_STACK)
        want = 1 << 30
        resource.setrlimit(resource.RLIMIT_STACK, (want if hard == resource.RLIM_INFINITY else min(want, hard), hard))
    except (ValueError, OSError):
        pass


def run_wasm(env, wasm, timeout):
    if env.runtime == "wasmtime":
        cmd = [os.environ.get("WASMTIME", "wasmtime"), "run", "-W", "tail-call=y,simd=y,max-wasm-stack=268435456", "--dir", "/tmp", "--dir", ".", "--env", "PATH", wasm]
        try:
            r = subprocess.run(cmd, capture_output=True, timeout=timeout, text=True, errors="replace")
        except subprocess.TimeoutExpired:
            return 142, "", "timed out"
        status = r.returncode
        if "wasm trap" in r.stderr or "unreachable" in r.stderr:
            status = 134
        return status, r.stdout, r.stderr
    cmd = ["node", "--no-warnings", "--stack-size=900000", RUN_MJS, "--dir", "/tmp", "--dir", ".", wasm]
    try:
        r = subprocess.run(cmd, capture_output=True, timeout=timeout, text=True, errors="replace", preexec_fn=big_stack)
    except subprocess.TimeoutExpired:
        return 142, "", "timed out"
    return r.returncode, r.stdout, r.stderr


def judge(env, directory, key, path, index):
    h = parse_header(path)
    key, status, why = judge_header(env, directory, key, path, index, h)
    if status == "FAIL" and h.get("open"):
        return key, "OPEN", "excused by `open: %s`: %s" % (h["open"], why)
    return key, status, why


def judge_header(env, directory, key, path, index, h):
    expect = h.get("expect", "")
    roots = [os.path.normpath(os.path.join(os.path.dirname(path), r)) for r in h.get("roots", "").split()]
    wasm = os.path.join(env.scratch, "c%d.wasm" % index)
    cmd = [env.fibc, "build", path, "-o", wasm, "--target", "wasm32-wasi"] + env.extra
    for r in roots:
        cmd += ["-I", r]
    try:
        b = subprocess.run(cmd, capture_output=True, timeout=env.timeout, text=True, errors="replace", env=dict(os.environ, FIB_RESULT_STDOUT="1"))
    except subprocess.TimeoutExpired:
        return key, "FAIL", "build timed out"
    if expect == "reject":
        if b.returncode == 0:
            return key, "FAIL", "built, but the header says reject"
        return key, "PASS", ""
    if b.returncode != 0:
        msg = (b.stderr or b.stdout).strip().splitlines()
        return key, "FAIL", "build: " + (msg[0] if msg else "status %d" % b.returncode)[:200]
    status, out, err = run_wasm(env, wasm, env.timeout)
    try:
        os.remove(wasm)
    except OSError:
        pass
    if expect == "trap":
        want = h.get("trap", "")
        if status == 0:
            return key, "FAIL", "ran to the end, the header says trap"
        if ("trap: " + want) not in err and want not in err:
            return key, "FAIL", "trap message differs: " + err.strip().splitlines()[-1][:160] if err.strip() else "no trap message"
        return key, "PASS", ""
    if expect == "accept":
        if status != 0:
            tail = err.strip().splitlines()
            return key, "FAIL", "status %d: %s" % (status, tail[-1][:160] if tail else "")
        lines = out.strip().splitlines()
        want = h.get("result", "")
        if not lines or lines[-1].strip() != want:
            return key, "FAIL", "result %r, header says %s" % (lines[-1].strip() if lines else "", want)
        return key, "PASS", ""
    return key, "FAIL", "header has no expect"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dir")
    ap.add_argument("--fibc", required=True)
    ap.add_argument("--only", nargs="*", default=[])
    ap.add_argument("-j", type=int, default=2)
    ap.add_argument("--expected")
    ap.add_argument("--results")
    ap.add_argument("--scratch")
    ap.add_argument("--runtime", default="node", choices=["node", "wasmtime"])
    ap.add_argument("--no-simd", action="store_true", help="build with FIB_TARGET_CPU=generic-nosimd128 semantics (features -simd128)")
    ap.add_argument("--extra", nargs="*", help="more arguments for `build`")
    ap.add_argument("--timeout", type=int, default=30)
    a = ap.parse_args()
    cases = list_cases(a.dir)
    if a.only:
        cases = [c for c in cases if any(c[0].startswith(p) for p in a.only)]
    own = a.scratch is None
    a.scratch = a.scratch or tempfile.mkdtemp(prefix="wasm-suite-", dir=os.path.expanduser("~/.cache/fibber-scratch"))
    os.makedirs(a.scratch, exist_ok=True)
    env = Env(a)
    t0 = time.time()
    with ThreadPoolExecutor(max_workers=a.j) as pool:
        rows = list(pool.map(lambda ic: judge(env, a.dir, ic[1][0], ic[1][1], ic[0]), enumerate(cases)))
    if own:
        shutil.rmtree(a.scratch, ignore_errors=True)
    passed = [r for r in rows if r[1] == "PASS"]
    openc = [r for r in rows if r[1] == "OPEN"]
    failed = [r for r in rows if r[1] == "FAIL"]
    base = a.dir.rstrip("/")
    known = {}
    if a.expected and os.path.exists(a.expected):
        for line in open(a.expected, encoding="utf-8"):
            line = line.strip()
            if line and not line.startswith("#"):
                parts = line.split(None, 2)
                known[parts[0]] = parts[1] if len(parts) > 1 else "FAIL"
    unexpected = [r for r in failed if "%s/%s" % (base, r[0]) not in known]
    fixed = [r for r in passed if "%s/%s" % (base, r[0]) in known]
    if a.results:
        with open(a.results, "w", encoding="utf-8") as f:
            for k, s, why in rows:
                f.write("%s/%s\t%s\t%s\n" % (base, k, s, why))
    for k, s, why in failed:
        mark = "known" if "%s/%s" % (base, k) in known else "NEW"
        print("%-5s %s/%s: %s  %s" % (s, base, k, why, "" if mark == "known" else "<-- not in the expected list"))
    for k, s, why in fixed:
        print("FIXED %s/%s passes but is in the expected list" % (base, k))
    print("%s: %d cases, %d pass, %d fail (%d expected, %d new), %d open (excused by their headers), %d fixed, %.0fs, -j %d" %
          (base, len(rows), len(passed), len(failed), len(failed) - len(unexpected), len(unexpected), len(openc), len(fixed), time.time() - t0, a.j))
    sys.exit(0 if not unexpected and not fixed else 1)


if __name__ == "__main__":
    main()
