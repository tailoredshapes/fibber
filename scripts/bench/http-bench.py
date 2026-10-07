#!/usr/bin/env python3
"""scripts/bench/http-bench.py: the HTTP-1 measurements (docs/shootout/http.md). Medians of N runs (default 5).
  FIBC=/path/to/fibc scripts/bench/http-bench.py [--runs 5] [--quick]
Builds scripts/bench/http-load.fib (the native load generator and the native server under test), starts the servers on ephemeral ports and measures:
  1. keep-alive GET, 1 connection and 64 connections, small body (100 B) and 1 MiB: native server vs python3 http.server, with the native client as load
  2. 10,000 sequential GETs (one keep-alive connection): the native client vs the curl CLI, both against the native server
  3. resident memory per idle connection of the native server (RSS growth over 256 idle connections)
No tuning is done before correctness; the numbers are whatever the first correct implementation gives."""
import os, re, socket, statistics, subprocess, sys, time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
FIBC = os.environ.get("FIBC", "fibc")
OUT = os.path.expanduser("~/.cache/fibber-scratch/http-bench")
os.makedirs(OUT, exist_ok=True)
runs = int(sys.argv[sys.argv.index("--runs") + 1]) if "--runs" in sys.argv else 5
quick = "--quick" in sys.argv
LOAD = os.path.join(OUT, "http-load")
subprocess.run([FIBC, "build", os.path.join(ROOT, "scripts/bench/http-load.fib"), "-I", os.path.join(ROOT, "lib"), "-o", LOAD], check=True)


class Server:
    def __init__(self, cmd):
        self.p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.port = int(self.p.stdout.readline())

    def rss_kb(self):
        for line in open(f"/proc/{self.p.pid}/status"):
            if line.startswith("VmRSS"):
                return int(line.split()[1])

    def stop(self):
        self.p.stdin.close()
        try:
            self.p.wait(10)
        except subprocess.TimeoutExpired:
            self.p.kill()


def native():
    return Server([LOAD, "--serve", "0", "100", "1048576"])


def python():
    return Server([sys.executable, os.path.join(ROOT, "scripts/bench/http-py-server.py")])


def load(port, path, conns, per):
    r = subprocess.run([LOAD, f"http://127.0.0.1:{port}{path}", str(conns), str(per)], capture_output=True, text=True, timeout=600)
    m = dict(kv.split("=") for kv in r.stdout.split())
    return {k: int(v) for k, v in m.items()}


def median(xs):
    return statistics.median(xs)


print(f"# medians of {runs} runs; native client load generator; this box ({os.cpu_count()} cpus, load {os.getloadavg()[0]:.1f})")
print("| server | body | conns | requests | rps | p50 us | p90 us | p99 us |")
print("|---|---|---:|---:|---:|---:|---:|---:|")
for name, make in (("native", native), ("python http.server", python)):
    for path, label, per in (("/small", "100 B", 4000), ("/big", "1 MiB", 100)):
        for conns in (1, 64):
            n = per if conns == 1 else max(10, per // 8)
            if quick:
                n = max(10, n // 10)
            srv = make()
            rs = [load(srv.port, path, conns, n) for _ in range(runs)]
            srv.stop()
            print(f"| {name} | {label} | {conns} | {rs[0]['requests']} | {median([r['rps'] for r in rs])} | {median([r['p50_us'] for r in rs])} | "
                  f"{median([r['p90_us'] for r in rs])} | {median([r['p99_us'] for r in rs])} |", flush=True)

print("\n| 10,000 sequential GETs, one keep-alive connection | server | seconds (median) |")
print("|---|---|---:|")
N = 1000 if quick else 10000
for sname, make in (("native", native), ("python http.server", python)):
    srv = make()
    ts = []
    for _ in range(runs):
        t0 = time.time()
        subprocess.run(["curl", "-s", "-o", "/dev/null", f"http://127.0.0.1:{srv.port}/small?n=[1-{N}]"], check=True)
        ts.append(time.time() - t0)
    print(f"| curl CLI (URL globbing) | {sname} | {median(ts):.2f} |", flush=True)
    ts = []
    for _ in range(runs):
        t0 = time.time()
        subprocess.run([LOAD, f"http://127.0.0.1:{srv.port}/small", "1", str(N)], capture_output=True, check=True)
        ts.append(time.time() - t0)
    print(f"| native client | {sname} | {median(ts):.2f} |", flush=True)
    srv.stop()

srv = native()
base = srv.rss_kb()
socks = []
for _ in range(256):
    s = socket.create_connection(("127.0.0.1", srv.port))
    s.sendall(b"GET /small HTTP/1.1\r\nHost: x\r\n\r\n")
    s.recv(4096)
    socks.append(s)
time.sleep(0.5)
held = srv.rss_kb()
print(f"\nmemory: native server RSS {base} kB with no connection, {held} kB with 256 idle keep-alive connections: {(held - base) * 1024 / 256:.0f} bytes per connection")
for s in socks:
    s.close()
srv.stop()
