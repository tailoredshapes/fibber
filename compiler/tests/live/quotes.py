#!/usr/bin/env python3
"""compiler/tests/live/quotes.py: a swap of a function whose program holds statics numbered per program (a quoted form `@q.N`: the
`expect` macro of fib.test.core quotes its form; a closure constant `@clo.N`; a vtable `@vt.N`) leaves every later input right
(serve.livesplit `split`, serve.livesess, emit.statics `program-static?`; docs/design/live-meshql.md 10), over a real socket against
test-app.fib, one fresh app per scenario.
usage: quotes.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
The defect this pins (found by LT2's test, fixed after it): LV3's split exported every non-code global of a swap's cut into the session's
base, `q.N` among them, so the next input's cut declared its own `q.N` (another form, of another shape) from the base: a call of a
base scenario's `run` after the swap died with `fatal signal 11 at 0xe`, and a swap from a three-binding scenario to a one-binding one
answered `no match clause matched`. What is tested, each in its own app:
  - edited-then-base: `(defspecs edited ..)` at the REPL, its scenario run, the same `defspecs` with the expectation changed (a swap),
    the new scenario run, then two scenarios of the base's `specs` run: every run answers its two steps and the app lives;
  - edited-twice-no-run: the same swap with no run before it (the swap alone poisoned the base's cut);
  - specs-long-short and specs-short-long: `specs` itself swapped between a scenario of three `given` bindings and one of one, a run
    after each swap;
  - after the swap the `test` op of the base's spec still passes its rows, and the app ends through the socket with status 0.
The mutant of scripts/mutant-live.sh that must make this fail: quote-shared (a quoted form is not a program-numbered static)."""
import json, os, shutil, socket, subprocess, sys, tempfile, time

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
app = os.path.join(here, 'test-app.fib')
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')   # a unix socket path is at most 100 bytes (serve.sock)
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')))
env.pop('FIB_LIVE', None)
failed = []

def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)

def limit():
    if sys.platform != 'linux': return
    import resource
    try: resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
    except (ValueError, OSError): pass

class App:
    def __init__(self, path):
        self.out = tempfile.NamedTemporaryFile(prefix='live-out-', delete=False)
        self.err = tempfile.NamedTemporaryFile(prefix='live-err-', delete=False)
        self.p = subprocess.Popen([fibc, 'run', '--live', path], stdout=self.out, stderr=self.err, cwd=root, env=env, preexec_fn=limit)
        self.sock = None
        for _ in range(1800):
            if self.p.poll() is not None: break
            for line in open(self.err.name).read().splitlines():
                if line.startswith('fibc: live at '): self.sock = line[len('fibc: live at '):]
            if self.sock and os.path.exists(self.sock) and 'app: ready' in self.stdout(): break
            time.sleep(0.1)
    def stdout(self): return open(self.out.name).read()
    def stderr(self): return open(self.err.name).read()
    def alive(self): return self.p.poll() is None
    def wait(self, secs=60):
        try: return self.p.wait(secs)
        except subprocess.TimeoutExpired: self.p.kill(); return None
    def clean(self):
        for f in (self.out.name, self.err.name):
            try: os.unlink(f)
            except OSError: pass

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

RUN = '(count ((. (nth (. (nth (%s) 0) scenarios) %d) run) 1))'    # a scenario's `run`: its StepRun rows (a given and a then: 2)
def edited(n): return '(defspecs edited (feature "Edited" (scenario "reads the REPL def" (given [l @limit]) (then (expect = %d l)))))' % n
SPECS_LONG = '(defspecs specs (feature "Slow" (scenario "two seconds" (given [a (slow-twice 3) z (u/sleep-ns 20) b (slow-twice 3)]) (then (expect = a b)))))'
SPECS_SHORT = '(defspecs specs (feature "Slow" (scenario "two seconds" (given [a (slow-twice 4)]) (then (expect = 8 a)))))'
LIMIT = '(def limit: (Atom i64) (atom 10))'
# each scenario: the inputs in order, each with what its answer must hold (`defined` or `swapped` names, or the printed value), and
# the scenario of the program's `specs` the test op selects at the end (the base's, or the swapped one's)
scenarios = {
    'edited-then-base': ([(LIMIT, {'defined': ['limit']}), (edited(10), {'defined': ['edited']}), (RUN % ('edited', 0), {'value': '2'}),
                          (edited(12), {'swapped': ['edited']}), (RUN % ('edited', 0), {'value': '2'}),
                          (RUN % ('specs', 0), {'value': '2'}), (RUN % ('specs', 1), {'value': '2'})], 'a-positive-number'),
    'edited-twice-no-run': ([(LIMIT, {'defined': ['limit']}), (edited(10), {'defined': ['edited']}), (edited(12), {'swapped': ['edited']}),
                             (RUN % ('specs', 0), {'value': '2'}), (RUN % ('specs', 1), {'value': '2'})], 'a-positive-number'),
    'specs-long-short': ([(RUN % ('specs', 0), {'value': '2'}), (SPECS_LONG, {'swapped': ['specs']}), (RUN % ('specs', 0), {'value': '2'}),
                          (SPECS_SHORT, {'swapped': ['specs']}), (RUN % ('specs', 0), {'value': '2'})], 'two-seconds'),
    'specs-short-long': ([(RUN % ('specs', 0), {'value': '2'}), (SPECS_SHORT, {'swapped': ['specs']}), (RUN % ('specs', 0), {'value': '2'}),
                          (SPECS_LONG, {'swapped': ['specs']}), (RUN % ('specs', 0), {'value': '2'})], 'two-seconds'),
}

def statuses(a):
    return [r['status'] for r in (a.get('report') or {}).get('results', [])]

def run_scenario(name, inputs, only):
    a = App(app)
    check(name + ': started', a.sock is not None and a.alive(), 'stderr: %s' % a.stderr()[-400:])
    if a.sock is None or not a.alive(): a.p.kill(); return
    c = Client(a.sock)
    for text, want in inputs:
        s = c.eval(text)
        if not s or not a.alive():
            check('%s: %s' % (name, text[:60]), False, 'the app died: %s' % a.stderr()[-120:].strip().replace('\n', ' / '))
            a.p.kill(); a.clean(); return
        check('%s: %s' % (name, text[:60]), s.get('ok') is True and all(s.get(k) == v for k, v in want.items()), s)
    t = c.ask(op='test', only=only)
    check(name + ': the spec still runs through the test op', t and t.get('ok') is True and statuses(t) == ['pass'], t)
    check(name + ': stop', c.eval('(reset! stop true)').get('ok'))
    code = a.wait(60)
    check(name + ': program ended', code == 0 and a.stdout().splitlines()[-1] == '0', (code, a.stdout()[-200:], a.stderr()[-200:]))
    c.close(); a.clean()

for name in sorted(scenarios): run_scenario(name, *scenarios[name])
shutil.rmtree(runtime, ignore_errors=True)
print('live quotes: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
