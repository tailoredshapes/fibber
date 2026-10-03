#!/usr/bin/env python3
"""usage: prof-report.py BINARY PROF.out [N]
Self time per function (leaf frame) and inclusive time (any of the 6 frames), as percent of samples.
Offsets of the executable are resolved with `nm -n`; other modules keep their exported names."""
import subprocess, sys, bisect, collections

exe, out = sys.argv[1], sys.argv[2]
top = int(sys.argv[3]) if len(sys.argv) > 3 else 15
syms = []
for line in subprocess.run(["nm", "-n", exe], capture_output=True, text=True).stdout.splitlines():
    p = line.split()
    if len(p) == 3 and p[1] in "tTwW":
        syms.append((int(p[0], 16), p[2]))
addrs = [a for a, _ in syms]

def name(frame):
    head, _, off = frame.rpartition("+")
    n, _, mod = head.rpartition("@")
    if n:
        return n
    if mod and mod != exe.rsplit("/", 1)[-1]:
        return "[" + mod + "]"
    i = bisect.bisect_right(addrs, int(off, 16)) - 1
    return syms[i][1] if i >= 0 else "?"

self_t, incl = collections.Counter(), collections.Counter()
total = 0
for line in open(out):
    fr = [name(f) for f in line.strip().split(" < ") if f]
    if not fr:
        continue
    total += 1
    self_t[fr[0]] += 1
    for f in set(fr):
        incl[f] += 1
print(f"{total} samples (1 ms each)")
print("self:")
for f, c in self_t.most_common(top):
    print(f"  {100*c/total:5.1f}%  {f}")
print("inclusive (within 6 frames):")
for f, c in incl.most_common(top):
    print(f"  {100*c/total:5.1f}%  {f}")

# phases: the innermost frame that belongs to a compiler pass module (f.<pass>.. or l.f.<pass>..) names the phase
import re
pat = re.compile(r"^(?:l\.)?(?:f|m)\.(syntax|expand|macros|types|own|emit|driver|lair)\.")
ph = collections.Counter()
for line in open(out):
    fr = [name(f) for f in line.strip().split(" < ") if f]
    if not fr:
        continue
    hit = next((pat.match(f).group(1) for f in fr if pat.match(f)), None)
    ph[hit or "(none: runtime/library only)"] += 1
print("phase (innermost pass-module frame, 32-frame heuristic stack):")
for f, c in ph.most_common():
    print(f"  {100*c/total:5.1f}%  {f}")

# optional 4th argument: a function-name substring; the nearest compiler/user frames above it (library and runtime frames skipped)
if len(sys.argv) > 4:
    target, cs, tot = sys.argv[4], collections.Counter(), 0
    skip = ("f.fib.", "fib.", "drop", "share", "[", "l.f.fib.", "m.fib")
    for line in open(out):
        fr = [name(f) for f in line.strip().split(" < ") if f]
        if any(target in f for f in fr):
            tot += 1
            cs[" < ".join([f for f in fr if not f.startswith(skip)][:2])] += 1
    print(f"callers of *{target}* ({tot} samples):")
    for k, v in cs.most_common(10):
        print(f"  {v:6d}  {k}")
