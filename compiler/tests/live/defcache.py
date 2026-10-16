#!/usr/bin/env python3
"""compiler/tests/live/defcache.py: the checks of the constant defs' values kept across the programs of a live session (package LE1 of
docs/design/live-incremental.md 2.2 and 4; emit.defs.cache, emit.program `DefCache`, serve.livesess `program-tables`) over a real socket
against defcache-app.fib, an HTTP app whose handler reads six constant defs of every shape (an array, a struct with a string, an Option,
a scalar, a str, a named function as a value) and forty constant records.
usage: defcache.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested, with the cache on (the default), FIB_LIVE_EMIT=verify and FIB_LIVE_EMIT=off, one app each, the three side by side:
  - the handler swapped three times reads every def the same through every generation, and expression inputs read the same values
    (after every swap with the cache on; after the last under verify and off, where each input is a whole emit);
    the def holding a function value (LE2's guard: never kept) follows a swap of its function through the running program's closure;
  - a required module (`extra`, written beside the app) whose constant defs are edited on disk between two inputs: the next input reads
    the new definitions (the key is the body name AND the lowered initialiser: a changed def is a miss, never a stale replay);
  - the answers of the three apps agree byte for byte (values, HTTP bodies, codes; every input the verify and off apps were given), so the cache changes nothing but time, and under
    `verify` no input is refused (every replayed value equals its fresh evaluation). The `extra` scenario under `off` also pins the
    session-wide numbering of constants (emit.program `DefCache.next`): before LE1 an input numbered its constants from 0, and the
    required module's `tags` took the name of a constant the base defines, so the input read fib.http's header names instead;
  - the emit of a swap with the cache on is under a third of the same swap's with the cache off (the design's bound: the defs are the
    emit), and under an absolute bound of 2,500 ms (measured 130 to 190 ms on lilnas with the cache on, 1,900 to 2,300 off, at load 2 to 4).
The mutants of scripts/mutant-live.sh that must make this fail: defcache-stale (the edited module's old values are read),
defcache-no-items (a replayed value names a constant the program lacks: the input is refused), defcache-verify-blind (under verify a
wrong replay is not refused: the edited module's old values are read under verify)."""
import json, os, shutil, socket, subprocess, sys, tempfile, threading, time, urllib.request, urllib.error

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
app_src = os.path.join(here, 'defcache-app.fib')
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')   # a unix socket path is at most 100 bytes (serve.sock)
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')))
env.pop('FIB_LIVE', None); env.pop('FIB_LIVE_EMIT', None)
failed = []
BOUND_MS = 2500

def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)

def limit():
    if sys.platform != 'linux': return
    import resource
    try: resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
    except (ValueError, OSError): pass

def free_port():
    s = socket.socket(); s.bind(('127.0.0.1', 0)); p = s.getsockname()[1]; s.close(); return p

EXTRA_A = '(ns extra)\n(def k: i64 5)\n(def tags: (Vec str) ["a" "b"])\n'
EXTRA_B = '(ns extra)\n(def k: i64 6)\n(def tags: (Vec str) ["a" "b" "c"])\n'

class App:
    """defcache-app.fib started with `fibc run --live` in a directory of its own (the app, `extra.fib` beside it, the stop file)."""
    def __init__(self, mode):
        self.dir = tempfile.mkdtemp(prefix='defcache-%s-' % (mode or 'on'), dir=runtime)
        shutil.copy(app_src, os.path.join(self.dir, 'app.fib'))
        self.extra = os.path.join(self.dir, 'extra.fib')
        open(self.extra, 'w').write(EXTRA_A)
        self.stop = os.path.join(self.dir, 'stop')
        self.port = free_port()
        e = dict(env)
        if mode: e['FIB_LIVE_EMIT'] = mode
        self.out = open(os.path.join(self.dir, 'out'), 'w'); self.err = open(os.path.join(self.dir, 'err'), 'w')
        self.p = subprocess.Popen([fibc, 'run', '--live', os.path.join(self.dir, 'app.fib'), '-I', self.dir, '--', '--port', str(self.port), '--stop', self.stop],
                                  stdout=self.out, stderr=self.err, cwd=root, env=e, preexec_fn=limit)
        self.sock = None
        for _ in range(1800):
            if self.p.poll() is not None: break
            for line in open(self.err.name).read().splitlines():
                if line.startswith('fibc: live at '): self.sock = line[len('fibc: live at '):]
            if self.sock and os.path.exists(self.sock) and 'app: listening' in self.stdout(): break
            time.sleep(0.1)
    def stdout(self): return open(self.out.name).read()
    def stderr(self): return open(self.err.name).read()
    def alive(self): return self.p.poll() is None
    def end(self):
        open(self.stop, 'w').write('stop\n')
        try: return self.p.wait(60)
        except subprocess.TimeoutExpired: self.p.kill(); return None

class Client:
    def __init__(self, path, timeout=600):
        self.s = socket.socket(socket.AF_UNIX); self.s.settimeout(timeout); self.s.connect(path); self.buf = b''
    def line(self):
        while b'\n' not in self.buf:
            d = self.s.recv(1 << 20)
            if not d: return None
            self.buf += d
        line, self.buf = self.buf.split(b'\n', 1)
        return line
    def ask(self, **req):
        self.s.sendall((json.dumps(req) + '\n').encode())
        line = self.line()
        return json.loads(line) if line else None
    def eval(self, text): return self.ask(op='eval', text=text) or {}
    def close(self): self.s.close()

def get(port, path):
    """status and body of GET path"""
    try:
        with urllib.request.urlopen('http://127.0.0.1:%d%s' % (port, path), timeout=20) as r: return r.status, r.read().decode()
    except urllib.error.HTTPError as e: return e.code, e.read().decode()
    except Exception as e: return 0, str(e)

DEFS = '[2 3 5 7 11 13] ada 36 7 3 hello 30 40 3360 27 k39'
READS = ['primes', '(. owner name)', '(. owner age)', 'maybe', 'limit', 'greeting', '(scorer 3)', '(count rows)',
         '(rows-sum)', '(rows-tagged)', '(. row39 key)']
HANDLES = ['(defun handle (req: Request) -> Response (route req "v%d"))' % n for n in (1, 2, 3)]

def transcript(mode):
    """The inputs against one app started under FIB_LIVE_EMIT=mode: the record of every answer (what the three apps must agree on),
    the emit_ms of the three swaps, and the checks of this app."""
    tag = mode or 'on'
    a = App(mode)
    check('%s: started' % tag, a.sock is not None and a.alive() and 'app: listening on %d' % a.port in a.stdout(), 'stderr: %s stdout: %s' % (a.stderr()[-800:], a.stdout()))
    if a.sock is None or not a.alive(): a.p.kill(); return None, [], a
    c = Client(a.sock)
    rec, emits = [], []
    def note(key, value):
        rec.append((key, value)); return value
    h = c.ask(op='hello')
    check('%s: hello generation 0' % tag, h.get('generation') == 0, h)
    note('base /defs', get(a.port, '/defs'))
    check('%s: the base reads the defs' % tag, rec[-1][1] == (200, 'v0 ' + DEFS), rec[-1][1])
    expected = dict(zip(READS, ['[2 3 5 7 11 13]', '"ada"', '36', '7', '3', '"hello"', '30', '40', '3360', '27', '"k39"']))
    for n, text in enumerate(HANDLES, 1):
        s = c.eval(text)
        ok = s.get('ok') and s.get('swapped') == ['handle'] and s.get('generation') == n
        check('%s: swap %d of handle' % (tag, n), ok, s)
        if ok: emits.append(s['swap']['emit_ms'])
        note('swap %d /defs' % n, get(a.port, '/defs'))
        check('%s: generation %d reads the same defs' % (tag, n), rec[-1][1] == (200, 'v%d %s' % (n, DEFS)), rec[-1][1])
        # the expressions after every swap with the cache on, after the last one only under verify and off: there every input is a
        # whole emit (about 3 s here, 10 s on an M1 Ultra), and 33 of them made the stamp outlast 900 s on the hosted runners
        if mode is None or n == len(HANDLES):
            for r in READS:
                v = c.eval(r)
                note('swap %d read %s' % (n, r), (v.get('ok'), v.get('value'), v.get('code')))
            reads = [(k[len('swap %d read ' % n):], v) for k, v in rec if k.startswith('swap %d read ' % n)]
            check('%s: expression inputs read every def after swap %d' % (tag, n), len(reads) == len(READS) and all(v == (True, expected[k], None) for k, v in reads), reads)
    # the def holding a function value follows a swap of its function (the closure constant's code is the stub)
    s = c.eval('(defun score (n: i64) -> i64 (* n 100))')
    check('%s: swap score' % tag, s.get('swapped') == ['score'] and s.get('generation') == 4, s)
    note('score /score/3', get(a.port, '/score/3')); note('score /defs', get(a.port, '/defs'))
    check('%s: the function value in a def runs the new body' % tag, rec[-2][1] == (200, '300') and rec[-1][1] == (200, 'v3 ' + DEFS.replace(' 30 ', ' 300 ')), rec[-2:])
    note('score (scorer 3)', c.eval('(scorer 3)').get('value'))
    check('%s: an expression reads the def holding the function value' % tag, rec[-1][1] == '300', rec[-1][1])
    # a required module's constant defs edited on disk between two inputs: the next input reads the new definitions
    r = c.eval('(require [extra :as e])')
    check('%s: require extra' % tag, r.get('ok') is True, r)
    note('extra A', (c.eval('e/k').get('value'), c.eval('e/tags').get('value')))
    check('%s: extra as written' % tag, rec[-1][1] == ('5', '["a" "b"]'), rec[-1][1])
    open(a.extra, 'w').write(EXTRA_B)
    ek, et = c.eval('e/k'), c.eval('e/tags')
    note('extra B', (ek.get('value'), et.get('value')))
    check('%s: extra edited on disk is read anew' % tag, rec[-1][1] == ('6', '["a" "b" "c"]'), (ek, et))
    note('extra B /defs', get(a.port, '/defs'))
    check('%s: the program still reads its own defs' % tag, rec[-1][1] == (200, 'v3 ' + DEFS.replace(' 30 ', ' 300 ')), rec[-1][1])
    st = c.ask(op='status')
    check('%s: status generation 4' % tag, st.get('generation') == 4, st)
    c.close()
    code = a.end()
    check('%s: program ended' % tag, code == 0 and a.stdout().splitlines()[-1] == '0', (code, a.stdout()[-300:], a.stderr()[-300:]))
    return rec, emits, a

# The three apps run side by side (each in a directory of its own, on a port of its own): one after another they were over the
# 900 s of a stamp on a 4-vCPU hosted runner (CI of 37dabc68), where `verify` and `off` lower the whole program for every input.
def run_mode(mode, results):
    t0 = time.monotonic()
    try: results[mode] = transcript(mode)
    except Exception as e:   # an answer missing (None) or not JSON: the app died or hung; the record is lost
        check('%s: the transcript ran' % (mode or 'on'), False, repr(e)); results[mode] = (None, [], None)
    print('%s: %.0f s' % (mode or 'on', time.monotonic() - t0))

results = {}
threads = [threading.Thread(target=run_mode, args=(m, results)) for m in (None, 'verify', 'off')]
for t in threads: t.start()
for t in threads: t.join()
on, on_emits, _ = results[None]
verify, verify_emits, _ = results['verify']
off, off_emits, _ = results['off']
def agree(name, a, b):
    """every answer of `b` is the cache-on app's answer to the same input (`b` has the reads of the last swap only)"""
    da = dict(a or [])
    keys = [k for k, _ in (b or [])]
    check(name, a is not None and b is not None and len(set(keys)) == len(keys) and all(k in da and da[k] == v for k, v in b),
          [(k, da.get(k), v) for k, v in (b or []) if da.get(k) != v][:5])

agree('cache on and off agree on every answer', on, off)
agree('verify agrees with the cache on', on, verify)
print('emit_ms of the three swaps: on %s, verify %s, off %s' % (on_emits, verify_emits, off_emits))
if len(on_emits) == 3 and len(off_emits) == 3:
    on_med, off_med = sorted(on_emits)[1], sorted(off_emits)[1]
    check('a swap emits in under a third of the time of a swap with the cache off', on_med * 3 < off_med, (on_med, off_med))
    check('a swap emits in under %d ms with the cache on' % BOUND_MS, max(on_emits) < BOUND_MS, on_emits)
else:
    check('the swaps reported their emit time', False, (on_emits, off_emits))
shutil.rmtree(runtime, ignore_errors=True)
print('live defcache: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
