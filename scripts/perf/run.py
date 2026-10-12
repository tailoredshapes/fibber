#!/usr/bin/env python3
"""run.py OUT.json -- CMD... : wall, user/sys, maxrss, RSS timeline, timestamped stderr, loadavg."""
import sys, os, time, subprocess, threading, json, resource
out = sys.argv[1]
cmd = sys.argv[sys.argv.index('--') + 1:]
t0 = time.monotonic()
T0ABS = t0
lines = []
samples = []
p = subprocess.Popen(cmd, stderr=subprocess.PIPE, stdout=subprocess.DEVNULL, text=True)


def rd():
    for l in p.stderr:
        lines.append((round(time.monotonic() - t0, 2), l.rstrip()))


th = threading.Thread(target=rd)
th.start()


def rss(pid):
    try:
        for l in open(f'/proc/{pid}/status'):
            if l.startswith('VmRSS'):
                return int(l.split()[1])
    except Exception:
        return None


load0 = os.getloadavg()
while p.poll() is None:
    samples.append((round(time.monotonic() - t0, 2), rss(p.pid)))
    time.sleep(0.25)
th.join()
wall = time.monotonic() - t0
ru = resource.getrusage(resource.RUSAGE_CHILDREN)
res = dict(mono0=T0ABS, cmd=cmd, rc=p.returncode, wall=round(wall, 2), user=round(ru.ru_utime, 2), sys=round(ru.ru_stime, 2),
           maxrss_kib=ru.ru_maxrss, load_before=load0, load_after=os.getloadavg(), stderr=lines, rss=samples)
json.dump(res, open(out, 'w'))
print(f"wall={wall:.2f} user={ru.ru_utime:.2f} sys={ru.ru_stime:.2f} cpu/wall={(ru.ru_utime+ru.ru_stime)/wall:.2f} "
      f"maxrss={ru.ru_maxrss/1024:.0f}MiB load={load0[0]:.2f}->{os.getloadavg()[0]:.2f} rc={p.returncode}")
