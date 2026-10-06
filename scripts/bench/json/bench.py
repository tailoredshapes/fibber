#!/usr/bin/env python3
"""Python's json (and orjson when installed): parse and serialise, MB/s, median of 5 batches. usage: bench.py FILE REPS"""
import json, sys, time, statistics
path, reps = sys.argv[1], int(sys.argv[2])
raw = open(path, 'rb').read()
text = raw.decode()
def mbs(f, nbytes):
    v = []
    for _ in range(5):
        t0 = time.perf_counter()
        for _ in range(reps):
            f()
        v.append(nbytes * reps / (time.perf_counter() - t0) / 1e6)
    return statistics.median(v)
doc = json.loads(text)
out = json.dumps(doc, separators=(',', ':'))
print('python json parse     %8.1f MB/s' % mbs(lambda: json.loads(text), len(raw)))
print('python json serialise %8.1f MB/s (of output bytes)' % mbs(lambda: json.dumps(doc, separators=(',', ':')), len(out)))
try:
    import orjson
    print('orjson parse          %8.1f MB/s' % mbs(lambda: orjson.loads(raw), len(raw)))
    print('orjson serialise      %8.1f MB/s' % mbs(lambda: orjson.dumps(doc), len(out)))
except ImportError:
    print('orjson not installed')
