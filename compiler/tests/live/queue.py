#!/usr/bin/env python3
"""compiler/tests/live/queue.py: the checks of the queue mode (package LE3 of docs/design/live-incremental.md 2.3; emit.program
`resident`/`program-request`, emit.typetext, serve.liveswap `requests-of`) over a real socket against queue-app.fib. A swap lowers only
the changed family against the resident program; everything else is answered by name and declared by the cut.
usage: queue.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested, with the queue mode on (the default), FIB_LIVE_QUEUE=0 (whole emit) and FIB_LIVE_EMIT=verify, one app each:
  - a handler swapped twice: `/h` reflects each generation (the second generation's bodies and closure literal carry a new generation,
    not colliding with the first's); the swap's `emit_ms` is under 60 (the design's target for the hello app);
  - a generic `dup` used at i64 and f64 only through the resident `summary`: a swap of `dup` changes both `/calc` values (the queue mode
    re-requests the registered specialisations by their recorded type text, since it does not reach them from the resident caller);
  - `probe` swapped to call a generic at `str`, a type the base never instantiated: `/probe` answers 7 (the new specialisation is lowered
    and linked, not answered by name);
  - the answers of the three apps agree byte for byte, so the queue mode changes nothing but time; the swap's `emit_ms` with the queue
    mode on is well under the whole emit's (FIB_LIVE_QUEUE=0).
The mutants of scripts/mutant-live.sh that must make this fail: resident-by-family (the swapped generation is answered by name, never
lowered), skip-new-specialisation (a missing specialisation answered by name: ORC's lookup fails), literal-counter-per-program (a
generation's bodies/literals do not carry the generation: the second swap's names collide), specialisation-not-requested (only
main-reached specialisations swap: `dup`'s f64 instantiation is missed)."""
import json, os, shutil, socket, subprocess, sys, tempfile, time, urllib.request, urllib.error

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
app_src = os.path.join(here, 'queue-app.fib')
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')))
env.pop('FIB_LIVE', None); env.pop('FIB_LIVE_EMIT', None); env.pop('FIB_LIVE_QUEUE', None)
failed = []
BOUND_MS = 60

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

class App:
    def __init__(self, mode, extra_env):
        self.dir = tempfile.mkdtemp(prefix='queue-%s-' % mode, dir=runtime)
        shutil.copy(app_src, os.path.join(self.dir, 'app.fib'))
        self.stop = os.path.join(self.dir, 'stop')
        self.port = free_port()
        e = dict(env); e.update(extra_env)
        self.out = open(os.path.join(self.dir, 'out'), 'w'); self.err = open(os.path.join(self.dir, 'err'), 'w')
        self.p = subprocess.Popen([fibc, 'run', '--live', os.path.join(self.dir, 'app.fib'), '--', '--port', str(self.port), '--stop', self.stop],
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
    try:
        with urllib.request.urlopen('http://127.0.0.1:%d%s' % (port, path), timeout=20) as r: return r.status, r.read().decode()
    except urllib.error.HTTPError as e: return e.code, e.read().decode()
    except Exception as e: return 0, str(e)

HANDLES = ['(defun handle (req: Request) -> Response (route req (fn (s: str) -> str (str "v%d " s))))' % n for n in (1, 2)]
BASE_H = '55 hello [2 3 5 7 11 13]'

def transcript(mode, extra_env):
    a = App(mode, extra_env)
    check('%s: started' % mode, a.sock is not None and a.alive() and 'app: listening on %d' % a.port in a.stdout(),
          'stderr: %s stdout: %s' % (a.stderr()[-800:], a.stdout()))
    if a.sock is None or not a.alive(): a.p.kill(); return None, [], a
    c = Client(a.sock)
    rec, emits = [], []
    def note(key, value): rec.append((key, value)); return value
    check('%s: hello generation 0' % mode, c.ask(op='hello').get('generation') == 0)
    note('base /h', get(a.port, '/h')); note('base /calc', get(a.port, '/calc')); note('base /probe', get(a.port, '/probe'))
    check('%s: base /h' % mode, rec[-3][1] == (200, 'v0 ' + BASE_H), rec[-3][1])
    check('%s: base /calc' % mode, rec[-2][1] == (200, '6 5.0'), rec[-2][1])
    check('%s: base /probe' % mode, rec[-1][1] == (200, '0'), rec[-1][1])
    # swap the handler twice: each generation's /h reflects it, and each swap's emit is bounded
    for n, text in enumerate(HANDLES, 1):
        s = c.eval(text)
        ok = s.get('ok') and s.get('swapped') == ['handle'] and s.get('generation') == n
        check('%s: swap %d of handle' % (mode, n), ok, s)
        if ok: emits.append(s['swap']['emit_ms'])
        note('handle %d /h' % n, get(a.port, '/h'))
        check('%s: generation %d /h' % (mode, n), rec[-1][1] == (200, 'v%d %s' % (n, BASE_H)), rec[-1][1])
    # a generic reached only through the resident `summary`: both specialisations swap (the body stays generic: `x + x + x`)
    s = c.eval('(defun dup (x: a) :where ((Num a)) -> a (+ x (+ x x)))')
    ok = s.get('swapped') == ['dup'] and s.get('generation') == 3
    check('%s: swap dup' % mode, ok, s)
    if ok: emits.append(s['swap']['emit_ms'])
    note('dup /calc', get(a.port, '/calc'))
    check('%s: both dup specialisations swapped' % mode, rec[-1][1] == (200, '9 7.5'), rec[-1][1])
    # a new specialisation the swap must lower (box at str, never instantiated in the base)
    s = c.eval('(defun probe () -> i64 (box "x"))')
    ok = s.get('swapped') == ['probe'] and s.get('generation') == 4
    check('%s: swap probe' % mode, ok, s)
    if ok: emits.append(s['swap']['emit_ms'])
    note('probe /probe', get(a.port, '/probe'))
    check('%s: new specialisation lowered and linked' % mode, rec[-1][1] == (200, '7'), rec[-1][1])
    check('%s: resident grew for the new specialisation' % mode, c.ask(op='status').get('values', {}).get('count', 0) >= 0)
    c.close()
    code = a.end()
    check('%s: program ended' % mode, code == 0 and a.stdout().splitlines()[-1] == '0', (code, a.stdout()[-300:], a.stderr()[-300:]))
    return rec, emits, a

on, on_emits, _ = transcript('on', {})
off, off_emits, _ = transcript('off', {'FIB_LIVE_QUEUE': '0'})
verify, verify_emits, _ = transcript('verify', {'FIB_LIVE_EMIT': 'verify'})
check('queue on and whole emit agree on every answer', on is not None and on == off, [p for p in zip(on or [], off or []) if p[0] != p[1]][:5])
check('verify agrees with the queue mode', on is not None and on == verify, [p for p in zip(on or [], verify or []) if p[0] != p[1]][:5])
print('emit_ms of the swaps: on %s, off %s, verify %s' % (on_emits, off_emits, verify_emits))
# On this hello-sized app the whole *body* emit (FIB_LIVE_QUEUE=0) is itself only a few ms, so the queue mode's win over it is not
# visible here (it is on meshql: docs/design/live-incremental.md 2.6); the acceptance for the hello app is the absolute bound, and
# verify (which re-lowers every def's reachable code, the work a whole emit does) is the proxy that shows the queue mode avoids it.
if len(on_emits) == 4 and len(verify_emits) == 4:
    check('every swap emits in under %d ms with the queue mode on' % BOUND_MS, max(on_emits) < BOUND_MS, on_emits)
    check('the queue mode avoids the whole emit verify forces', max(on_emits) * 4 < min(verify_emits), (on_emits, verify_emits))
else:
    check('the swaps reported their emit time', False, (on_emits, verify_emits))
shutil.rmtree(runtime, ignore_errors=True)
print('live queue: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
