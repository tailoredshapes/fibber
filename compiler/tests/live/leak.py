#!/usr/bin/env python3
"""compiler/tests/live/leak.py: the resident size of a live session over 200 `load`s of one module (docs/design/live-incremental.md 8).
usage: leak.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
The program: eight library-sized modules m0..m7 (30 functions each), `model` (the module that is swapped) and the main module, which calls
`model/val` in a loop. 200 swaps of `val`; the VmRSS of the session is read at swaps 100 and 200. The engine kept, for every text of `model`,
the expander's entry and the persistent checker's state of the module and of every module after it (each a copy of the library's tables): about
8 MB a swap here, 820 MB in the last 100 (measured 2026-10-22). The bound is 40 MB for the last 100 swaps (measured after the fix: 9 MB, the
generations that stay resident until a `retire`), so a regression to even a tenth of the old growth fails."""
import json, os, shutil, socket, subprocess, sys, tempfile, time

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
work = tempfile.mkdtemp(prefix='live-leak-', dir=os.environ.get('TMPDIR') or None)
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')   # a unix socket path is at most 100 bytes (serve.sock)
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')))
env.pop('FIB_LIVE', None)
failed = []
LIMIT_MB = 40
SWAPS = 200

def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)

def limit():
    import resource
    try: resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
    except (ValueError, OSError): pass

os.mkdir(os.path.join(work, 'src'))
for i in range(8):
    body = '\n'.join('(defun f%d-%d (x: i64) -> i64 (+ (* x %d) (count (map (fn (y: i64) (+ y x)) [1 2 3 %d]))))' % (i, j, j + 1, j) for j in range(30))
    open(os.path.join(work, 'src', 'm%d.fib' % i), 'w').write('(ns m%d)\n%s\n' % (i, body))
model = os.path.join(work, 'src', 'model.fib')
def write_model(v): open(model, 'w').write('(ns model)\n(defun val () -> i64 %d)\n' % v)
write_model(1)
open(os.path.join(work, 'app.fib'), 'w').write('''(ns main (:require %s [model :as md] [fib.unix :as u]))
(def stop: (Atom bool) (atom false))
(defun main () -> i64
  (do (println "app: ready")
      (loop () (if @stop () (do (md/val) (u/sleep-ns 20000000) (recur))))
      (println "app: stopped")
      0))
''' % ' '.join('[m%d :as m%d]' % (i, i) for i in range(8)))

out = open(os.path.join(work, 'out'), 'w'); err = open(os.path.join(work, 'err'), 'w')
p = subprocess.Popen([fibc, 'run', '--live', 'app.fib', '-I', 'src'], stdout=out, stderr=err, cwd=work, env=env, preexec_fn=limit)
sock = None
for _ in range(1800):
    if p.poll() is not None: break
    for line in open(err.name).read().splitlines():
        if line.startswith('fibc: live at '): sock = line[len('fibc: live at '):]
    if sock and os.path.exists(sock) and 'app: ready' in open(out.name).read(): break
    time.sleep(0.1)
check('session starts', sock is not None and p.poll() is None, 'no socket: ' + open(err.name).read()[-300:])
if not failed:
    s = socket.socket(socket.AF_UNIX); s.settimeout(300); s.connect(sock); f = s.makefile('rw')
    def ask(**r):
        f.write(json.dumps(r) + '\n'); f.flush(); return json.loads(f.readline())
    def rss(): return int([l for l in open('/proc/%d/status' % p.pid) if l.startswith('VmRSS')][0].split()[1]) / 1024
    at = {}
    ok = True
    for i in range(1, SWAPS + 1):
        write_model(i + 1)
        r = ask(op='load', file=model)
        if not r.get('ok'): ok = False; check('swap %d' % i, False, str(r)[:300]); break
        if i in (100, SWAPS): at[i] = rss()
    if ok:
        check('swaps accepted', True)
        v = ask(op='eval', text='(md/val)')
        check('the last swap is the one running', str(v.get('value', v.get('result', v))).find(str(SWAPS + 1)) >= 0, str(v)[:200])
        grow = at[SWAPS] - at[100]
        print('rss at swap 100: %.0f MB, at swap %d: %.0f MB, growth over the last 100: %.1f MB (limit %d)' % (at[100], SWAPS, at[SWAPS], grow, LIMIT_MB))
        check('growth of the last 100 swaps bounded', grow < LIMIT_MB, '%.1f MB' % grow)
    try: ask(op='eval', text='(reset! stop true)')
    except Exception: pass
    p.wait(30) if p.poll() is None else None
if p.poll() is None: p.kill()
shutil.rmtree(work, ignore_errors=True); shutil.rmtree(runtime, ignore_errors=True)
sys.exit(1 if failed else 0)
