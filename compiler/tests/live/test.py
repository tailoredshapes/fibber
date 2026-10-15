#!/usr/bin/env python3
"""compiler/tests/live/test.py: the checks of the `test` op of a live session (docs/design/live-meshql.md 3, package LT2; serve.livetest,
serve.livewords, serve.live, serve.liveclient, `--with` of expand.modules) over a real socket against test-app.fib, a live app that is
its own spec, with test-spec.fib loaded beside it by `--with`.
usage: test.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested: a whole run (the JSON answer has the five rows with the statuses `fibc test` gives them: pass, fail, trap, pass, pass;
status 1; generation_start = generation_end, not stale); `--only` the failing row; the function under test swapped (an `eval` of the
`defun`) and `:test` (rerun-last) through the terminal client: the row passes and the rows are printed as `fibc test` prints them with
the `replay:` line as a `:test` command; a step edit (the spec function defined again, a scenario reading a `def` made at the REPL) seen
by the next run (the registry is made again per run); `isolate: fork` and `timeout` answer `test/NoFork`; a selection that matches
nothing is `test/NothingSelected`; `--list` through the sink; the two-second scenario with a swap of `slow-twice` sent at one second:
`stale: true` and the row shows both generations' answers, `status` during the run answers within 100 ms (the run is on a thread of its
own), a second `test` during it is `busy` with the elapsed time; `--with`: the spec file beside the app is `spec.test-spec`, `spec:` runs
it (two rows), as does `:test test-spec.fib`; the app ran on (its hits) and ends through the socket.
The mutants of scripts/mutant-live.sh that must make this fail: stale-never, rerun-old-registry, test-on-attach-thread, sink-shared-run."""
import json, os, re, shutil, socket, subprocess, sys, tempfile, time

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
app = os.path.join(here, 'test-app.fib')
spec = os.path.join(here, 'test-spec.fib')
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
    def __init__(self, path, args=()):
        self.out = tempfile.NamedTemporaryFile(prefix='live-out-', delete=False)
        self.err = tempfile.NamedTemporaryFile(prefix='live-err-', delete=False)
        self.p = subprocess.Popen([fibc, 'run', '--live'] + list(args) + [path], stdout=self.out, stderr=self.err, cwd=root, env=env, preexec_fn=limit)
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
    def send(self, **req): self.s.sendall((json.dumps(req) + '\n').encode())
    def answer(self):
        line = self.line()
        return json.loads(line) if line else None
    def ask(self, **req):
        self.send(**req)
        return self.answer()
    def eval(self, text): return self.ask(op='eval', text=text) or {}
    def value(self, text): return self.eval(text).get('value')
    def close(self): self.s.close()

def terminal(sock, lines):
    """the terminal client's output lines for the inputs (the raw client is closed meanwhile: the program serves one connection at a time)"""
    global c
    c.close()
    r = subprocess.run([fibc, 'repl', '--connect', sock], input=''.join(l + '\n' for l in lines) + ':quit\n', capture_output=True, text=True, cwd=root, env=env, timeout=600, preexec_fn=limit)
    c = Client(sock)
    return r.stdout.splitlines(), r

def matches(name, got, want):
    """`want` is one regex (anchored) per expected line"""
    ok = len(got) == len(want) and all(re.fullmatch(w, g) for g, w in zip(got, want))
    check(name, ok, '\n  got:  %s\n  want: %s' % (got, want))

def statuses(a):
    return [r['status'] for r in (a.get('report') or {}).get('results', [])]

def row(a, text):
    for r in (a.get('report') or {}).get('results', []):
        if r['text'] == text: return r
    return {}

a = App(app, ['--with', spec])
check('started', a.sock is not None and a.alive() and 'app: ready' in a.stdout(), 'stderr: %s stdout: %s' % (a.stderr()[-800:], a.stdout()))
if a.sock is None or not a.alive(): a.p.kill(); sys.exit(1)
c = Client(a.sock)
h = c.ask(op='hello')
check('hello names the --with file', h.get('generation') == 0 and h.get('with') == [spec], h)

# ---- a whole run: the rows as fibc test gives them, status 1, not stale
t = c.ask(op='test', id=1)
check('whole run: ok, status 1, five rows', t.get('ok') is True and t.get('id') == 1 and t.get('status') == 1 and statuses(t) == ['pass', 'fail', 'trap', 'pass', 'pass'], t)
check('whole run: the document is schema 1 with the seed', (t.get('report') or {}).get('schema') == 1 and t['report'].get('seed') == 1 and t['report']['summary'] == {'total': 5, 'pass': 3, 'fail': 1, 'trap': 1, 'timeout': 0}, t.get('report'))
check('whole run: not stale, generation 0 to 0', t.get('generation_start') == 0 and t.get('generation_end') == 0 and t.get('stale') is False and t.get('ms', -1) >= 2000, t)
check('whole run: the failing row says why', row(t, 'zero').get('steps', [{}])[-1].get('actual') == '"pos"' and row(t, 'a trapping step').get('message', '').startswith('trap: '), row(t, 'zero'))
check('whole run: threads_after is the app\'s', t.get('threads_after') == 0, t.get('threads_after'))
check('the scenarios printed to the program\'s output', 'hello from a scenario' in a.stdout(), a.stdout())
check('the scenarios bumped the app\'s atom', c.value('@hits') == '4', c.value('@hits'))

# ---- --only the failing row; nothing selected; --list; the refusals
t = c.ask(op='test', only='zero')
check('--only zero: one failing row', t.get('ok') is True and statuses(t) == ['fail'] and t.get('status') == 1, t)
t = c.ask(op='test', only='nothing-like-this')
check('nothing selected', t.get('ok') is False and t.get('code') == 'test/NothingSelected' and t.get('status') == 2 and "no scenario matches 'nothing-like-this'" in t['error']['message'], t)
t = c.ask(op='test', list=True)
check('--list through the sink', t.get('ok') is True and t.get('report') is None and t.get('text', '').startswith('Classify/a-positive-number\n') and 'Slow/two-seconds\n' in t.get('text', ''), t)
for name, req, code in [('isolate fork', dict(isolate='fork'), 'test/NoFork'), ('timeout', dict(timeout=5), 'test/NoFork'), ('format', dict(format='tap'), 'test/Format'),
                        ('a spec that is not loaded', dict(spec='nope-spec.fib'), 'type/Resolve')]:
    t = c.ask(op='test', **req)
    check('refused: ' + name, t.get('ok') is False and t.get('code') == code and t.get('hint'), t)
check('unchanged after the refusals', c.ask(op='status').get('generation') == 0 and c.ask(op='status')['tests'] == {'running': False, 'last': {'scenarios': 0, 'ms': c.ask(op='status')['tests']['last']['ms'], 'status': 0}}, c.ask(op='status').get('tests'))

# ---- fix the function under test, rerun the last selection through the terminal client: the row passes, the rows print as fibc test's
out, r = terminal(a.sock, [':test --only zero',
                           '(defun classify (n: i64) -> str (cond (< n 0) "neg" (= n 0) "zero" :else "pos"))',
                           ':test'])
# the positions are the session's text's (`<repl>:L:C`: the input's program is the REPL's re-assembled text, live-repl.md 9 (g))
matches('fix and rerun', out, [r'; attached to .*test-app\.fib \(pid %d, generation 0, catching\); \d+ threads?, pool \d+ workers' % a.p.pid,
                               r'Classify', r'  FAIL  zero', r'        Given h = \(bump\)', r'      > Then "zero" = \(classify 0\)', r'          expected: "zero"', r'          actual:   "pos"',
                               r'          at <repl>:\d+:\d+', r'        id Classify/zero  \(<repl>:\d+:\d+\)', r'',
                               r'1 scenarios: 0 pass, 1 fail, 0 trap, 0 timeout \(seed 1\)', r"replay: :test --only 'Classify/zero' --seed 1",
                               r'; 1 scenario in \d+ ms \(generation 0\)',
                               r'; swapped classify \(generation 1\): check \d+ ms, emit \d+ ms, jit \d+ ms',
                               r'Classify', r'  pass  zero', r'', r'1 scenarios: 1 pass, 0 fail, 0 trap, 0 timeout \(seed 1\)',
                               r'; 1 scenario in \d+ ms \(generation 1\)'])
t = c.ask(op='test', last=True)
check('rerun-last keeps the selection', t.get('ok') is True and statuses(t) == ['pass'] and t.get('generation_start') == 1, t)
check('the swap is seen by the server too', c.value('(classify 0)') == '"zero"')

# ---- a swap during a run: status answers meanwhile, a second test is busy, the answer is stale and shows both generations
gen = c.ask(op='status')['generation']
c.send(op='test', id='slow', only='two-seconds')
time.sleep(0.6)
t0 = time.time(); st = c.ask(op='status'); dt = time.time() - t0
check('status during a run answers within 100 ms', st and st.get('generation') == gen and 'tests' in st and st['tests']['running'] is True and st['tests']['id'] == 'slow' and st['tests']['elapsed_ms'] > 0 and dt < 0.1, (st, dt))
b = c.ask(op='test', id='second')
check('a second test is busy, with the elapsed time', b and b.get('ok') is False and b['error']['code'] == 'busy' and b.get('id') == 'second' and b['running']['id'] == 'slow' and b['running']['elapsed_ms'] >= 500, b)
sw = c.eval('(defun slow-twice (n: i64) -> i64 (* 20 n))')
check('a swap lands during the run', sw.get('swapped') == ['slow-twice'] and sw.get('generation') == gen + 1, sw)
t = c.answer()
check('the run is stale and the row shows both generations', t and t.get('id') == 'slow' and t.get('stale') is True and t.get('generation_start') == gen and t.get('generation_end') == gen + 1
      and statuses(t) == ['fail'] and row(t, 'two seconds')['steps'][-1]['expected'] == '6' and row(t, 'two seconds')['steps'][-1]['actual'] == '60', t)
out, r = terminal(a.sock, [':test', ':status'])
matches('a rerun after the swap: both answers are the new one; status shows the last run', out,
        [r'; attached to .*', r'Slow', r'  pass  two seconds', r'', r'1 scenarios: 1 pass, 0 fail, 0 trap, 0 timeout \(seed 1\)', r'; 1 scenario in \d+ ms \(generation %d\)' % (gen + 1),
         r'; generation %d; generations kept \d+; modules kept \d+; threads: .*; values: \d+ resident; inputs \d+; tests: idle \(last: 1 scenarios, \d+ ms\)' % (gen + 1)])
c.send(op='test', id='slow2', only='two-seconds')
time.sleep(0.5)
c.close(); c = Client(a.sock)   # the connection that asked is gone: the run goes on, its answer is dropped and its result recorded
b = c.ask(op='test', id='third')
check('the run goes on without its connection', b.get('ok') is False and b['error']['code'] == 'busy', b)
time.sleep(2.5)
st = c.ask(op='status')
check('the run that lost its connection is recorded', st['tests']['running'] is False and st['tests']['last'] == {'scenarios': 1, 'ms': st['tests']['last']['ms'], 'status': 0}, st)

# ---- --with: the spec beside the app
t = c.ask(op='test', spec=spec)
check('--with: the spec file runs', t.get('ok') is True and statuses(t) == ['pass', 'fail'] and [r['suite'] for r in t['report']['results']] == ['With', 'With'], t)
t = c.ask(op='test', spec='spec.test-spec', only='loaded')
check('--with: by module name, --only', t.get('ok') is True and statuses(t) == ['pass'], t)
out, r = terminal(a.sock, [':test test-spec.fib --only wrong'])
matches(':test FILE', out, [r'; attached to .*', r'With', r'  FAIL  a wrong claim', r'        Given y = 2', r'      > Then 3 = y', r'          expected: 3', r'          actual:   2', r'          at .*test-spec\.fib:\d+:\d+',
                            r'        id With/a-wrong-claim  \(.*test-spec\.fib:\d+:\d+\)', r'', r'1 scenarios: 0 pass, 1 fail, 0 trap, 0 timeout \(seed 1\)',
                            r"replay: :test --only 'With/a-wrong-claim' --seed 1", r'; 1 scenario in \d+ ms \(generation \d+\)'])
t = c.ask(op='test', suites='(spec.test-spec/specs)', jobs=2, seed=7)
check('suites: an expression, the seed and the jobs', t.get('ok') is True and t['report']['seed'] == 7 and statuses(t) == ['pass', 'fail'], t)
out, r = terminal(a.sock, [':test --format json', ':test --timeout 3', ':test --isolate fork', ':test --bogus'])
matches('the client refuses the three words', out, [r'; attached to .*', r'; the report is rendered here.*', r'; a hung scenario cannot be killed.*', r'; fork isolation is refused.*', r'; unknown word --bogus.*'])

# ---- a step edit: a spec function made at the REPL, one scenario reading a def made there, run as `suites`; the step's text edited
# (the function defined again: a swap) and the rerun shows the edit: the registry (the closures) is made again by every run. Then the
# base's own spec once more: a swap of a function whose program quotes a form (`expect`) once broke every later cut of the base's
# `specs` (the per-program `@q.N` exported by the split; quotes.py pins it in full)
gen = c.ask(op='status')['generation']
d = c.eval('(def limit: (Atom i64) (atom 10))')
check('a def at the REPL', d.get('ok') and d.get('defined') == ['limit'], d)
s = c.eval('(defspecs edited (feature "Edited" (scenario "reads the REPL def" (given [l @limit]) (then (expect = 10 l)))))')
check('a spec function at the REPL', s.get('ok') and s.get('defined') == ['edited'] and s.get('generation') == gen, s)
t = c.ask(op='test', suites='(edited)')
check('suites: the new spec runs, reading the def', t.get('ok') is True and t.get('status') == 0 and [r['text'] for r in t['report']['results']] == ['reads the REPL def'] and statuses(t) == ['pass'], t)
s = c.eval('(defspecs edited (feature "Edited" (scenario "reads the REPL def" (given [l @limit]) (then (expect = 12 l)))))')
check('the step edited: the spec function swapped', s.get('ok') and s.get('swapped') == ['edited'] and s.get('generation') == gen + 1, s)
t = c.ask(op='test', last=True)
check('the edited step runs on the rerun', t.get('ok') is True and statuses(t) == ['fail'] and row(t, 'reads the REPL def')['steps'][-1]['expected'] == '12' and row(t, 'reads the REPL def')['steps'][-1]['actual'] == '10', t)
check('the REPL def changed, the step sees it', c.eval('(reset! limit 12)').get('ok') and statuses(c.ask(op='test', last=True)) == ['pass'])
t = c.ask(op='test', only='a-positive-number')
check('the base spec after the step edit: its scenario runs', t and t.get('ok') is True and statuses(t) == ['pass'], t)
check('the base scenario\'s run after the step edit answers', c.value('(count ((. (nth (. (nth (specs) 0) scenarios) 0) run) 1))') == '2' and a.alive(), a.stderr()[-200:])

# ---- the end of the program through the socket
check('stop', c.eval('(reset! stop true)').get('ok'))
code = a.wait(60)
out = a.stdout().splitlines()
check('program ended', code == 0 and out[0] == 'app: ready' and out[-2].startswith('app: stopped after ') and out[-1] == '0', (code, a.stdout()[-300:], a.stderr()[-300:]))
a.clean()
shutil.rmtree(runtime, ignore_errors=True)
print('live test: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
