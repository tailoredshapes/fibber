#!/usr/bin/env python3
"""Run native LIRO builds, edits, damaged artifacts and competing publishers."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import fcntl
import time
from concurrent.futures import ThreadPoolExecutor

ROOT = Path(__file__).resolve().parents[4]
STATS = re.compile(r"liro: hits=(\d+) misses=(\d+) compiled=(\d+)")
BASE = """(global g i64 (i64 7))
(define internal (left i64) () (block entry (ret (add (load i64 @g) (i64 1)))))
(define internal (right i64) () (block entry (ret (call @left))))
(define (main i32) () (block entry (ret (trunc i32 (call @right)))))
"""
INITIALIZERS = """(global shared.value i64 (i64 0))
(define internal (first void) () (block entry (store (i64 1) @shared.value) (ret)))
(define internal (second void) () (block entry (store (add (mul (load i64 @shared.value) (i64 10)) (i64 2)) @shared.value) (ret)))
(define (main i32) () (block entry (call @first) (call @second) (ret (trunc i32 (load i64 @shared.value)))))
"""
RECURSION = """(define internal (even i64) ((i64 n))
  (block entry (br (icmp eq n (i64 0)) done recur))
  (block done (ret (i64 8)))
  (block recur (ret (call @odd (sub n (i64 1))))))
(define internal (odd i64) ((i64 n))
  (block entry (br (icmp eq n (i64 0)) done recur))
  (block done (ret (i64 9)))
  (block recur (ret (call @even (sub n (i64 1))))))
(define (main i32) () (block entry (ret (trunc i32 (call @even (i64 4))))))
"""


TIMING = os.environ.get("LIRO_TIMING")


def run(argv, env, ok=True):
    t0 = time.monotonic()
    try:
        return _run(argv, env, ok)
    finally:
        if TIMING:
            print(f"time {time.monotonic() - t0:7.2f}s {Path(str(argv[0])).name} {argv[1] if len(argv) > 1 else ''}", flush=True)


def _run(argv, env, ok=True):
    p = subprocess.run([str(a) for a in argv], cwd=ROOT, env=env,
                       text=True, capture_output=True, timeout=240)
    if ok and p.returncode:
        raise AssertionError(f"{argv}: exit {p.returncode}\n{p.stdout}\n{p.stderr}")
    return p


def counts(p):
    m = STATS.search(p.stderr)
    assert m, f"missing actual backend counts: {p.stderr}"
    result = tuple(map(int, m.groups()))
    assert result[1] == result[2], result
    return result


def status(path, env, expected):
    p = run([path], env, ok=False)
    assert p.returncode == expected, (path, expected, p.returncode, p.stderr)


def write(path, text):
    path.write_text(text)
    return path


def build_probe(fibc, path, env, compiler=ROOT / "compiler"):
    lib = os.environ.get("LLVMLIB", "/usr/lib/llvm-21/lib")
    run([fibc, "build", ROOT / "compiler/tests/native/liro/api.fib", "-I", compiler,
         "-I", ROOT / "lib", "-L", lib, "-l", "LLVM-21", "-o", path],
        dict(env, FIB_LIRO_CACHE="off"))


def verify_cache(cache):
    entries = list(cache.glob("*/unit.liro.meta"))
    assert entries, "no published objects"
    for path in entries:
        m = json.loads(path.read_text())
        assert m["format"] == 1 and path.parent.name == m["key"]
        digest = hashlib.sha256(path.with_name("unit.liro").read_bytes()).hexdigest()
        assert digest == m["object"], path
        inputs = json.loads(m["inputs"])
        assert inputs["exports"] and "triple=" in inputs["settings"]


def cache_checks(probe, scratch, env):
    src = write(scratch / "base.lir", BASE)
    out = scratch / "program"
    cold = run([probe, "build", 2, src, out], env)
    n = counts(cold)[2]
    assert n > 1, "the test requires separate units"
    status(out, env, 8)
    warm = run([probe, "build", 2, src, out], env)
    assert counts(warm) == (n, 0, 0), warm.stderr
    assert "time llvm." not in warm.stderr, warm.stderr
    write(src, BASE.replace("(i64 1)", "(i64 2)"))
    edit = run([probe, "build", 2, src, out], env)
    h, _, compiled = counts(edit)
    assert h > 0 and 0 < compiled < n, edit.stderr
    status(out, env, 9)
    abi = BASE.replace("(i64 1)", "(i64 2)").replace("(left i64)", "(left i32)")
    abi = abi.replace("(ret (add (load i64 @g) (i64 2)))", "(ret (trunc i32 (add (load i64 @g) (i64 2))))")
    abi = abi.replace("(ret (call @left))", "(ret (zext i64 (call @left)))")
    write(src, abi)
    changed = run([probe, "build", 2, src, out], env)
    assert 0 < counts(changed)[2] < n and counts(changed)[0] > 0, changed.stderr
    status(out, env, 9)
    write(src, INITIALIZERS)
    run([probe, "build", 2, src, out], env)
    status(out, env, 12)
    warm = run([probe, "build", 2, src, out], env)
    assert counts(warm)[2] == 0, warm.stderr
    status(out, env, 12)
    for calls, expected in [("(call @second) (call @first)", 1),
                            ("(call @first) (call @second) (call @second)", 122)]:
        write(src, INITIALIZERS.replace("(call @first) (call @second)", calls))
        changed = run([probe, "build", 2, src, out], env)
        assert counts(changed)[2] > 0, changed.stderr
        status(out, env, expected)
    write(src, RECURSION)
    run([probe, "build", 2, src, out], env)
    status(out, env, 8)
    assert counts(run([probe, "build", 2, src, out], env))[2] == 0
    write(src, BASE.replace("(i64 1)", "(i64 2)"))
    for field, value in [("FIB_CATCH", "1"), ("FIB_TARGET_CPU", "generic")]:
        changed = run([probe, "build", 2, src, out], dict(env, **{field: value}))
        assert counts(changed)[0] == 0, (field, changed.stderr)
        status(out, env, 9)
    changed = run([probe, "build", 1, src, out], env)
    assert counts(changed)[0] == 0, changed.stderr
    entries = list(Path(env["FIB_LIRO_CACHE"]).glob("*/unit.liro"))
    for obj in entries:
        obj.write_bytes(b"corrupted object")
    rebuilt = run([probe, "build", 2, src, out], env)
    assert counts(rebuilt)[2] > 0, rebuilt.stderr
    status(out, env, 9)
    for meta in Path(env["FIB_LIRO_CACHE"]).glob("*/unit.liro.meta"):
        meta.write_text('{"format":999}')
    rebuilt = run([probe, "build", 2, src, out], env)
    assert counts(rebuilt)[0] == 0, rebuilt.stderr
    status(out, env, 9)
    print("ok cold/warm, body/initializer edits, CPU/catch/optimization isolation, corruption")


def artifact_checks(probe, scratch, env):
    src = write(scratch / "full.lir", BASE)
    obj, out = scratch / "full.liro", scratch / "linked"
    run([probe, "emit", 2, src, obj], env)
    run([probe, "link", 2, out, obj], env)
    status(out, env, 8)
    copied = scratch / "renamed object ' with spaces.liro"
    shutil.copyfile(obj, copied)
    shutil.copyfile(Path(str(obj) + ".meta"), Path(str(copied) + ".meta"))
    run([probe, "link", 2, out, copied], env)
    mismatch = run([probe, "link", 1, out, obj], env, ok=False)
    assert mismatch.returncode and "incompatible" in mismatch.stderr, mismatch.stderr
    duplicate = run([probe, "link", 2, out, obj, obj], env, ok=False)
    assert duplicate.returncode and "duplicate liro owner" in duplicate.stderr, duplicate.stderr
    copied.write_bytes(b"damaged")
    damaged = run([probe, "link", 2, out, copied], env, ok=False)
    assert damaged.returncode and "corrupted liro object" in damaged.stderr, damaged.stderr
    helper = write(scratch / "helper.lir", "(define (helper i64) () (block entry (ret (i64 9))))")
    main = write(scratch / "client.lir", "(declare helper i32 ())\n(define (main i32) () (block entry (ret (call @helper))))")
    a, b = scratch / "helper.liro", scratch / "client.liro"
    run([probe, "emit", 2, helper, a], env)
    run([probe, "emit", 2, main, b], env)
    missing = run([probe, "link", 2, out, b], env, ok=False)
    assert missing.returncode, "linking without helper's owner succeeded"
    incompatible = run([probe, "link", 2, out, a, b], env, ok=False)
    assert incompatible.returncode and "incompatible liro ABI" in incompatible.stderr, incompatible.stderr
    write(main, main.read_text().replace("helper i32", "helper i64").replace("(ret (call @helper))", "(ret (trunc i32 (call @helper)))"))
    run([probe, "emit", 2, main, b], env)
    run([probe, "link", 2, out, a, b], env)
    status(out, env, 9)
    native_src = write(scratch / "native.c", "int liro_native_value(void) { return 12; }\n")
    library = scratch / ("libliro_native.dylib" if sys.platform == "darwin" else "libliro_native.so")
    run(["cc", "-dynamiclib" if sys.platform == "darwin" else "-shared", "-fPIC", native_src, "-o", library], env)
    text = ";; link-lib: liro_native fixture\n(declare liro_native_value i32 ())\n(define (main i32) () (block entry (ret (call @liro_native_value))))"
    write(src, text)
    run([probe, "emit", 2, src, obj], env)
    metadata = json.loads(Path(str(obj) + ".meta").read_text())
    assert json.loads(metadata["inputs"])["libraries"] == ["liro_native"]
    native_env = dict(env, LIBRARY_PATH=str(scratch), LD_LIBRARY_PATH=str(scratch))
    run([probe, "link", 2, out, obj], native_env)
    status(out, native_env, 12)
    print("ok exported object round trip, renamed paths, checksum, owner and ABI checks")


def concurrency_checks(probe, scratch, env):
    src = write(scratch / "concurrent.lir", BASE)
    cache = scratch / "concurrent-cache"
    cache.mkdir()
    interrupted = cache / ".liro-interrupted"
    interrupted.mkdir()
    (interrupted / "unit.liro").write_bytes(b"incomplete")
    race_env = dict(env, FIB_LIRO_CACHE=str(cache))
    def build(i):
        out = scratch / f"concurrent-{i}"
        p = run([probe, "build", 2, src, out], race_env)
        status(out, env, 8)
        return counts(p)
    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(build, range(4)))
    verify_cache(cache)
    warm = run([probe, "build", 2, src, scratch / "race-warm"], race_env)
    assert counts(warm)[2] == 0, warm.stderr
    blocked = scratch / "killed-cache"
    blocked.mkdir()
    locks = []
    for entry in cache.glob("*/unit.liro.meta"):
        lock = (blocked / ("lock-" + entry.parent.name)).open("w")
        fcntl.flock(lock, fcntl.LOCK_EX)
        locks.append(lock)
    child = subprocess.Popen([str(probe), "build", "2", str(src), str(scratch / "killed")],
                             env=dict(env, FIB_LIRO_CACHE=str(blocked)), stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL, cwd=ROOT)
    try:
        deadline = time.monotonic() + 30
        while not list(blocked.glob(".liro-*/.liro-*/unit.liro.meta")):
            assert child.poll() is None and time.monotonic() < deadline, "writer did not reach publication"
            time.sleep(.02)
        child.kill()
        child.wait(timeout=10)
        assert not list(blocked.glob("*/unit.liro.meta")), "interrupted writer published an entry"
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=10)
        for lock in locks:
            lock.close()
    recovered = run([probe, "build", 2, src, scratch / "recovered"], dict(env, FIB_LIRO_CACHE=str(blocked)))
    assert counts(recovered)[0] == 0, recovered.stderr
    status(scratch / "recovered", env, 8)
    verify_cache(blocked)
    print("ok simultaneous publishers/readers, killed writer and unpublished incomplete entry")


def audit(stderr):
    live, floor = set(), 0
    for line in stderr.splitlines():
        words = line.split()
        if len(words) == 2 and words[0] == "I":
            floor, live = int(words[1]), set()
        elif len(words) == 3 and words[0] == "A":
            n = int(words[1])
            assert n not in live, f"duplicate allocation {line}"
            live.add(n)
        elif len(words) == 2 and words[0] == "F" and int(words[1]) > floor:
            n = int(words[1])
            assert n in live, f"free of an object not live {line}"
            live.remove(n)
    assert not live, f"leaked objects: {live}"


def identity_checks(fibc, scratch, env):
    lib = scratch / "reuse.fib"
    shutil.copyfile(ROOT / "compiler/tests/native/liro/reuse.fib", lib)
    common = """(let ((f (r/make 5)) (d (dyn r/Weight (r/Box 9))))
       (do (println (+ (f) (+ (r/bound (r/Box 9)) (+ (r/weight d) (r/dispatch (r/key))))))
           (println (r/pure 1)) (println (r/identity 7)) 0))"""
    a = write(scratch / "a.fib", "(ns main (:require [reuse :as r]))\n(defun main () -> i64 " + common + ")")
    b = write(scratch / "b.fib", "(ns main (:require [reuse :as r]))\n(defstruct Unrelated (n: i64))\n"
              "(defun main () -> i64 (do (println (. (Unrelated 1) n)) (println :before) " + common + "))")
    def build(src, name):
        out = scratch / name
        p = run([fibc, "build", src, "-I", scratch, "-o", out], env)
        executed = run([out], dict(env, FIB_TRACE="1"))
        audit(executed.stderr)
        return counts(p), executed.stdout
    cold, first = build(a, "identity-a")
    assert first == "34\n101\n7\n", first
    warm, again = build(a, "identity-a-warm")
    assert warm[2] == 0 and again == first, (warm, again)
    changed, second = build(b, "identity-b")
    assert second == "1\n:before\n" + first and changed[0] > 0 and changed[2] > 0, (changed, second)
    text = lib.read_text().replace("(defstruct Box (n: i64))", "(defstruct Box (padding: i64 n: i64))").replace("(Box n)", "(Box 0 n)")
    write(lib, text)
    a.write_text(a.read_text().replace("(r/Box 9)", "(r/Box 0 9)"))
    layout, result = build(a, "identity-layout")
    assert result == first and layout[2] > 0, (layout, result)
    print("ok changed type/keyword order, closures, dyn/bound dispatch, generic calls and layout edit; clean audits")


def unit_defs(p):
    return sum(int(m) for m in re.findall(r"liro: unit \d+ items=\d+ defs=(\d+)", p.stderr))


def parallel_checks(probe, scratch, env):
    """Cache-off partitioned builds are opt-in, parallel and deterministic (docs/design/compile-performance.md)."""
    src = write(scratch / "par.lir", BASE)
    off = dict(env, FIB_LIRO_CACHE="off")
    whole = run([probe, "build", 2, src, scratch / "par-whole"], off)
    assert "liro: unit" not in whole.stderr, "a cache-off build must stay whole-program unless opted in"
    status(scratch / "par-whole", env, 8)
    opted = dict(off, FIB_LIRO_PARTITION="1")
    outs = []
    # One output name in three directories: the macOS linker puts the output's file name in the code signature and derives LC_UUID from the
    # output path (the objects and their order are the same; 'par-1' against 'par-4' differed in those two fields alone on an M1).
    for name, jobs in [("par-1", "1"), ("par-4", "4"), ("par-4b", "4")]:
        (scratch / name).mkdir()
        out = scratch / name / "par"
        p = run([probe, "build", 2, src, out], dict(opted, FIB_LIRO_JOBS=jobs))
        assert len(re.findall(r"liro: unit \d+", p.stderr)) > 1, p.stderr
        status(out, env, 8)
        outs.append(out.read_bytes())
    assert outs[0] == outs[1] == outs[2], "unit order, not completion order, decides the link"
    other = scratch / "par-other"
    other.mkdir()
    run([probe, "build", 2, write(scratch / "par-other.lir", BASE.replace("(i64 7)", "(i64 9)")), other / "par"], dict(opted, FIB_LIRO_JOBS="4"))
    assert (other / "par").read_bytes() != outs[0], "the comparison must see a different program"
    plain = unit_defs(p)
    for extra in [{"FIB_LIRO_UNITS": "3"}, {"FIB_LIRO_SPLIT": "ns2"}, {"FIB_LIRO_UNITS": "64", "FIB_LIRO_SPLIT": "ns3"}]:
        p = run([probe, "build", 2, src, scratch / "par-x"], dict(opted, FIB_LIRO_JOBS="4", **extra))
        status(scratch / "par-x", env, 8)
        assert unit_defs(p) == plain, (extra, p.stderr)
    p = run([probe, "build", 2, src, scratch / "par-i"], dict(opted, FIB_LIRO_JOBS="4", FIB_LIRO_IMPORT="1000"))
    status(scratch / "par-i", env, 8)
    assert unit_defs(p) > plain, "copies of small callees must appear in the importing units"
    print("ok opt-in cache-off units compile in parallel, link in unit order, and match across jobs, splits and copies")


def phase(name, fn, *args):
    t0 = time.monotonic()
    fn(*args)
    if TIMING:
        print(f"phase {time.monotonic() - t0:7.2f}s {name}", flush=True)


def main():
    """check.py [FIBC] [all | checks | plant NAME]: the stamps of mk/tools.mk run the groups apart, `all` runs every one."""
    args = sys.argv[1:]
    explicit = args.pop(0) if args and args[0] not in GROUPS else "build/F"
    fibc = Path(os.environ.get("FIBC", explicit)).resolve()
    group = args[0] if args else "all"
    assert group in GROUPS, f"group {group}: one of {sorted(GROUPS)}"
    with tempfile.TemporaryDirectory(prefix="fibber-liro-test-", dir=os.environ.get("TMPDIR")) as directory:
        scratch = Path(directory)
        env = dict(os.environ, FIB_LIB=str(ROOT / "lib"), FIB_LIRO_CACHE=str(scratch / "cache"),
                   FIB_LIRO_STATS="1", FIB_TIME="1")
        if group in {"all", "checks"}:
            run_checks(fibc, scratch, env)
        from mutants import PLANTS, check_plant
        if group == "plant":
            assert len(args) == 2 and args[1] in PLANTS, f"plant NAME: one of {sorted(PLANTS)}"
            phase("plant " + args[1], check_plant, args[1], fibc, scratch, env, ROOT)
        if group == "all":
            from mutants import check_plants
            check_plants(fibc, scratch, env, ROOT)
    print("LIRO PASS " + " ".join(args or ["all"]))


GROUPS = {"all", "checks", "plant"}


def run_checks(fibc, scratch, env):
    probe = scratch / "api"
    phase('build_probe', build_probe, fibc, probe, env)
    phase('cache_checks', cache_checks, probe, scratch, env)
    phase('artifact_checks', artifact_checks, probe, scratch, env)
    phase('concurrency_checks', concurrency_checks, probe, scratch, env)
    phase('identity_checks', identity_checks, fibc, scratch, env)
    phase('parallel_checks', parallel_checks, probe, scratch, env)


if __name__ == "__main__":
    main()
