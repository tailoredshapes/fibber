#!/usr/bin/env python3
"""compiler/tests/live/attach.py: the checks of the attach socket of `fibc run --live` (docs/design/live-repl.md 2.1 to 2.5, package LV1;
serve.live, serve.livesess, serve.livesock, serve.liveclient) over a real socket against app.fib, a program with a worker thread and pool
work running while the inputs are evaluated.
usage: attach.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested: the socket (0700 directory, 0600 file, the path on standard error and in hello); hello; the design's atom transcript
(a `def` counter answers 1 then 2); a def read by three later inputs; a trap answered `failed` and the next input working; the
program's own defs read and changed live while its worker runs; the warm engine (LE5: hello's `warm`, the first input's figures);
type, inspect (LE4: a lookup in under 20 ms, `value: true`), an expression in under 150 ms, status; new functions, records and closures kept in an
atom; a function defined again is swapped (LV2: compiler/tests/live/swap.py is the swap's own test); a session def defined again (defonce keeps it, def
makes it again: LT4); the refusals (live/Main, the checker's); 1,000 malformed lines; a second client waiting; the terminal and
--json clients; `not-live`; a wrong uid refused (planted: FIB_LIVE_OWN_UID); a program that does not check; the end of the program
through the socket, its output and exit as `fibc run`'s."""
import json, os, random, shutil, signal, socket, stat, subprocess, sys, tempfile, time

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
app = os.path.join(here, 'app.fib')
# a unix socket path is at most 100 bytes here (serve.sock): the runtime directory is short, whatever TMPDIR is
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')
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
    """A program started with `fibc run --live`, its socket found from its standard error."""
    def __init__(self, path, extra_env=None, args=()):
        e = dict(env, **(extra_env or {}))
        self.out = tempfile.NamedTemporaryFile(prefix='live-out-', delete=False)
        self.err = tempfile.NamedTemporaryFile(prefix='live-err-', delete=False)
        self.p = subprocess.Popen([fibc, 'run', '--live', path] + list(args), stdout=self.out, stderr=self.err, cwd=root, env=e, preexec_fn=limit)
        self.sock = None
        for _ in range(1200):   # the library's expansion is about a second; a loaded machine more
            if self.p.poll() is not None: break
            for line in open(self.err.name).read().splitlines():
                if line.startswith('fibc: live at '): self.sock = line[len('fibc: live at '):]
            if self.sock and os.path.exists(self.sock): break
            time.sleep(0.1)
    def stdout(self): return open(self.out.name).read()
    def clean(self):
        for f in (self.out.name, self.err.name):
            try: os.unlink(f)
            except OSError: pass
    def stderr(self): return open(self.err.name).read()
    def wait(self, secs=30):
        try: return self.p.wait(secs)
        except subprocess.TimeoutExpired: self.p.kill(); return None

class Client:
    def __init__(self, path, timeout=300):
        self.s = socket.socket(socket.AF_UNIX); self.s.settimeout(timeout); self.s.connect(path); self.buf = b''
    def raw(self, data):
        self.s.sendall(data)
        return self.line()
    def line(self):
        while b'\n' not in self.buf:
            d = self.s.recv(1 << 20)
            if not d: return None
            self.buf += d
        line, self.buf = self.buf.split(b'\n', 1)
        return line
    def ask(self, **req):
        line = self.raw((json.dumps(req) + '\n').encode())
        return json.loads(line) if line else None
    def eval(self, text): return self.ask(op='eval', text=text)
    def value(self, text):
        a = self.eval(text)
        return a.get('value') if a else None
    def close(self): self.s.close()

# ---- the program and its socket
a = App(app)
check('started', a.sock is not None and a.p.poll() is None, 'no socket: %s' % a.stderr()[-500:])
if a.sock is None: a.p.kill(); sys.exit(1)
sockdir = os.path.dirname(a.sock)
check('socket path', a.sock == os.path.join(runtime, 'fib-live', '%d.sock' % a.p.pid), a.sock)
check('directory 0700', stat.S_IMODE(os.stat(sockdir).st_mode) == 0o700, oct(os.stat(sockdir).st_mode))
check('socket 0600', stat.S_IMODE(os.stat(a.sock).st_mode) == 0o600, oct(os.stat(a.sock).st_mode))
c = Client(a.sock)
h = c.ask(op='hello', id=1)
check('hello', h and h.get('ok') and h.get('pid') == a.p.pid and h.get('program', '').endswith('app.fib') and h.get('generation') == 0 and h.get('catching') is True and h.get('id') == 1, h)
check('hello version', h and isinstance(h.get('version'), str) and h['version'][0].isdigit(), h)
check('hello warm (LE5)', h and h.get('warm') is True, h)

# ---- values persist: the design's transcript (section 1 of docs/design/live-repl.md)
d = c.eval('(def counter: (Atom i64) (atom 0))')
check('def counter', d and d.get('ok') and d.get('defined') == ['counter'], d)
st = (d or {}).get('stats', {})   # LE5: the engine was warmed before the first connection: the first input expands the session module alone
check('first input warm (LE5)', st.get('modules', 0) > 1 and st.get('reused') == st['modules'] - 1 and st.get('check_ms', 9999) < 300, st)
check('counter 1', c.value('(do (swap! counter inc) @counter)') == '1')
check('counter 2', c.value('(do (swap! counter inc) @counter)') == '2')
check('def names', (c.eval('(def names: (Vec str) ["a" "b"])') or {}).get('defined') == ['names'])
check('read 1', c.value('(count names)') == '2')
check('read 2', c.value('(nth names 1)') == '"b"')
check('read 3', c.value('(str (nth names 0) (nth names 1) (count names))') == '"ab2"')
check('constant def', (c.eval('(def late: i64 5)') or {}).get('defined') == ['late'] and c.value('(+ late 1)') == '6')

# ---- a trap is a caught failure; the session goes on
t = c.eval('(nth [1 2] 5)')
check('trap failed', t and t.get('ok') is False and 'index out of range' in t.get('failed', '') and t.get('exit') == 134, t)
check('after trap', c.value('(+ 1 2)') == '3')
check('counter after trap', c.value('@counter') == '2')

# ---- the program's own state, live, while its worker and the pool run
t1 = int(c.value('@ticks')); time.sleep(0.3); t2 = int(c.value('@ticks'))
check('program atom read', t1 > 0 and t2 > t1, (t1, t2))
check('program atom changed', c.value('(swap! label (fn (_: str) -> str "busy"))') == '"busy"' and c.value('@label') == '"busy"')
check('pool work seen', int(c.value('@sums')) >= 0)
check('pool from input', c.value('(reduce + 0 (pmap (fn (x: i64) -> i64 (* x 2)) (vec (range 10))))') == '90')

# ---- type, inspect, status
check('type', (c.ask(op='type', expr='(+ 1 2)') or {}).get('type') == 'i64')
check('type of def', (c.ask(op='type', expr='counter') or {}).get('type') == '(Atom i64)')
def timed(**req):   # the best wall of three (ms) and the last answer: a loaded machine adds to one
    best, ans = None, None
    for _ in range(3):
        t0 = time.time(); ans = c.ask(**req); ms = (time.time() - t0) * 1000
        best = ms if best is None else min(best, ms)
    return best, ans
# LE4: inspect is a lookup in the live table and the last catalogue (no front end, no stats); `value: true` evaluates
ms, i = timed(op='inspect', name='handle')
check('inspect a function (LE4)', i and i.get('ok') and i.get('kind') == 'function' and i.get('generation') == 0 and 'f.handle.g0' in i.get('bodies', [])
      and i.get('scheme') == '(fn :send (Request) Response)' and i.get('summary') == ['borrowed'] and 'stats' not in i and 'value' not in i, i)
check('inspect under 20 ms (LE4)', ms < 20, ms)
ms, i = timed(op='inspect', name='names')
check('inspect a def (LE4)', i and i.get('ok') and i.get('kind') == 'def' and i.get('type') == '(Vec str)' and 'stats' not in i and 'value' not in i, i)
check('inspect a def under 20 ms', ms < 20, ms)
i = c.ask(op='inspect', name='names', value=True)
check('inspect value', i and i.get('value') == '["a" "b"]' and i.get('type') == '(Vec str)' and 'stats' in i, i)
i = c.ask(op='inspect', name='no-such-name')
check('inspect unknown', i and i.get('ok') is False and i.get('error', {}).get('code') == 'unknown-name', i)
ms, i = timed(op='eval', text='(count [1])')
check('expression under 150 ms (LE4)', i and i.get('value') == '1' and ms < 150, (ms, i))
s = c.ask(op='status')
check('status', s and s.get('generation') == 0 and s['generations']['count'] == 1 and s['values']['count'] >= 1 and ('def.slot' in ''.join(s['values']['resident']) or 'live.d.' in ''.join(s['values']['resident'])) and s.get('inputs', 0) > 5 and s.get('pid') == a.p.pid, s)   # names and late are constants: static data, no slot

# ---- new definitions after boot: a function, a record, a closure kept in an atom
check('defun', (c.eval('(defun twice (x: i64) -> i64 (* 2 x))') or {}).get('defined') == ['twice'] and c.value('(twice 21)') == '42')
check('defun uses def', (c.eval('(defun bump () -> i64 (swap! counter inc))') or {}).get('ok') and c.value('(bump)') == '3' and c.value('@counter') == '3')
check('defrecord', (c.eval('(defrecord Pt (x: i64 y: i64))') or {}).get('defined') == ['Pt'] and c.value('(Pt 3 4)') == '#main.Pt{:x 3, :y 4}')
check('closure kept', (c.eval('(def fs: (Atom (Option (fn (i64) i64))) (atom nil))') or {}).get('ok')
      and (c.eval('(reset! fs (some (fn (y: i64) -> i64 (+ y 100))))') or {}).get('ok') and c.value('(match @fs ((some f) (f 1)) (nil -1))') == '101')
check('def and expression', c.value('(def k: i64 (+ 20 1)) (* k 2)') == '42')
check('keyword map', c.value('(get {:a 1 :b 2} :b)') == '2')
check('unit expression', (lambda r: r and r.get('ok') and 'value' not in r)(c.eval('(println "from the input")')))

# ---- refusals: nothing changes
r = c.eval('(defun twice (x: i64) -> i64 (* 3 x))')
check('swapped', r and r.get('ok') and r.get('swapped') == ['twice'] and r.get('generation') == 1 and c.value('(twice 21)') == '63', r)
r = c.eval('(defonce counter: (Atom i64) (atom 9))')
check('defonce keeps the value', r and r.get('ok') and c.value('@counter') == '3', r)
r = c.eval('(def counter: (Atom i64) (atom 9))')
check('def again re-evaluated (LT4)', r and r.get('ok') and c.value('@counter') == '9' and c.value('(bump)') == '10', r)
check('def again: counter back to 3', (c.eval('(def counter: (Atom i64) (atom 3))') or {}).get('ok') and c.value('@counter') == '3')
r = c.eval('(defun main () -> i64 0)')
check('main refused', r and r.get('code') == 'live/Main', r)
r = c.eval('(+ 1 "a")')
check('type error', r and r.get('ok') is False and r.get('shown') and r.get('diagnostics') and 'type/' in r['diagnostics'][0]['code'], r)
r = c.eval('(defun broken (x: i64) -> i64 (+ x "a"))')
check('bad definition', r and r.get('ok') is False and r.get('shown') and (c.eval('(broken 1)') or {}).get('ok') is False, r)
r = c.eval('(+ 1 2) (+ 3 4)')
check('two expressions', r and r.get('code') == 'live/Input', r)
check('blank', (c.eval('   ') or {}).get('ok') is True)
r = c.eval('(+ 1')
check('unreadable', r and r.get('ok') is False and r.get('shown'), r)
check('still alive', c.value('(twice 2)') == '6')

# ---- malformed lines: the program survives them all
random.seed(7)
junk = [b'', b'{', b'[1,2]\n'[:-1], b'{"op":5}', b'{"id":3}', b'{"op":"nope","id":3}', b'{"op":"eval"}', b'{"op":"eval","text":5}',
        b'{"op":"type"}', b'{"op":"inspect","name":7}', bytes([0xff, 0xfe, 0x41]), b'\x00\x01', b'"just a string"', b'null']
answered = 0
for n in range(1000):
    line = junk[n % len(junk)] if n % 2 == 0 else bytes(random.randrange(32, 127) for _ in range(random.randrange(0, 60)))
    ans = c.raw(line + b'\n')
    if ans is None: break
    try: answered += json.loads(ans).get('ok') is False
    except ValueError: break
check('1000 malformed lines answered', answered == 1000, answered)
check('alive after junk', (c.ask(op='hello') or {}).get('pid') == a.p.pid and c.value('@counter') == '3')
check('unknown op', (c.ask(op='nope') or {}).get('error', {}).get('code') == 'unknown-op')
check('ping', (c.ask(op='ping') or {}).get('pong') is True)

# ---- a second client waits until the first is done (one request at a time)
c2 = Client(a.sock, timeout=20)
c2.s.sendall(b'{"op":"hello","id":"second"}\n')
time.sleep(0.5)
c2.s.settimeout(0.5)
try: early = c2.s.recv(100)
except socket.timeout: early = None
check('second client waits', early is None, early)
c.close()
c2.s.settimeout(20)
check('second client served', (json.loads(c2.line()) or {}).get('id') == 'second')
c2.close()

# ---- the clients
r = subprocess.run([fibc, 'repl', '--connect', str(a.p.pid), '--json'], input='{"op":"hello","id":7}\n{"op":"eval","text":"@counter"}\n{"op":"eval","text":"(nth [1] 5)"}\n',
                   capture_output=True, text=True, cwd=root, env=env, timeout=120, preexec_fn=limit)
lines = [json.loads(l) for l in r.stdout.splitlines()]
check('client --json', r.returncode == 0 and len(lines) == 3 and lines[0].get('id') == 7 and lines[1].get('value') == '3' and 'failed' in lines[2], (r.returncode, r.stdout, r.stderr))
r = subprocess.run([fibc, 'repl', '--connect', a.sock], input='(swap! counter inc)\n(nth [1] 5)\n:type counter\n(defun thrice (x: i64) -> i64 (* 3 x))\n(thrice 5)\n(defun thrice (x: i64) -> i64 x)\n:status\n:quit\n',
                   capture_output=True, text=True, cwd=root, env=env, timeout=120, preexec_fn=limit)
out = r.stdout.splitlines()
check('client terminal', r.returncode == 0 and out[0].startswith('; attached to ') and 'app.fib' in out[0] and 'catching' in out[0] and out[1] == '4' and out[2].startswith('; failed: ')
      and out[3] == '(Atom i64)' and out[4] == '; defined thrice' and out[5] == '15' and out[6].startswith('; swapped thrice (generation 2): check ')
      and out[7].startswith('; generation 2; generations kept 3;'), (r.returncode, r.stdout, r.stderr))
r = subprocess.run([fibc, 'repl', '--connect', '4000000', '--json'], input='{"op":"hello"}\n', capture_output=True, text=True, cwd=root, env=env, timeout=120, preexec_fn=limit)
check('not-live --json', r.returncode == 2 and r.stdout.count('\n') == 1 and json.loads(r.stdout).get('error', {}).get('code') == 'not-live', (r.returncode, r.stdout, r.stderr))
r = subprocess.run([fibc, 'repl', '--connect', '4000000'], input='(+ 1 2)\n', capture_output=True, text=True, cwd=root, env=env, timeout=120, preexec_fn=limit)
check('not-live terminal', r.returncode == 2 and 'not-live' in r.stderr, (r.returncode, r.stdout, r.stderr))

# ---- the end of the program, through the socket; its output and exit as fibc run's
c = Client(a.sock)
check('stop', (c.eval('(reset! stop true)') or {}).get('ok'))
code = a.wait(60)
out = a.stdout().splitlines()
check('program ended', code == 0 and out[0] == 'app: started' and out[-2].startswith('app: stopped after ') and 'label busy' in out[-2] and out[-1] == '0', (code, a.stdout(), a.stderr()[-300:]))
check('from the input printed', 'from the input' in out)
check('socket removed', not os.path.exists(a.sock))

# ---- a wrong uid is refused before a line is read (the planted uid)
b = App(app, {'FIB_LIVE_OWN_UID': str(os.getuid() + 1)})
check('planted uid: started', b.sock is not None)
if b.sock:
    s = socket.socket(socket.AF_UNIX); s.settimeout(20); s.connect(b.sock)
    try: got = s.recv(10)
    except socket.timeout: got = b'timeout'
    check('planted uid: refused', got == b'' and b.p.poll() is None, got)
    s.close()
    b.p.send_signal(signal.SIGTERM); b.wait(20)

# ---- a program that does not check: no socket, exit 3
bad = os.path.join(runtime, 'bad.fib')
open(bad, 'w').write('(ns main)\n(defun main () -> i64 (+ 1 "x"))\n')
before = set(os.listdir(os.path.join(runtime, 'fib-live')))
r = subprocess.run([fibc, 'run', '--live', bad], capture_output=True, text=True, cwd=root, env=env, timeout=300, preexec_fn=limit)
check('rejected program', r.returncode == 3 and 'rejected' in r.stderr and 'live at' not in r.stderr and set(os.listdir(os.path.join(runtime, 'fib-live'))) == before, (r.returncode, r.stderr[-300:]))

a.clean()
if b.sock: b.clean()
shutil.rmtree(runtime, ignore_errors=True)
print('live attach: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
