#!/usr/bin/env python3
"""compiler/tests/live/retire.py: what running state holds into a retired generation (docs/design/live-incremental.md 7; serve.livereach,
serve.liveretire, serve.livesplit) over a real socket against retire-app.fib.
usage: retire.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested: a closure made by generation 1 (`make-adder`) kept in an atom and in a def, and a thread spawned by generation 2
(`start-ticker`) running its closure; both families swapped again (3, 4). `bump`, which holds nothing, swapped twice (5, 6). The worker
parked at its pause point inside generation 7 of `step` (reached through a function value read from an atom, not a call the code
names), and `step` swapped while it is parked (8). One `retire` at quiescence; then: `bump`'s old generation is the one freed, the
generations that hold something are reported pinned, the program lives after the resume, the worker returns out of `step` and goes
on, the closures answer what their generation made them answer, the thread counts on. The mutant `retire-dynamic-unfollowed` of
scripts/mutant-live.sh must make this fail (the generation the worker is parked in is freed under it: the program dies on the resume)."""
import json, os, re, shutil, socket, subprocess, sys, tempfile, time

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')   # a unix socket path is at most 100 bytes (serve.sock)
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')))
env.pop('FIB_LIVE', None)
failed = []

def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)

class App:
    def __init__(self, path):
        self.out = tempfile.NamedTemporaryFile(prefix='live-out-', delete=False)
        self.err = tempfile.NamedTemporaryFile(prefix='live-err-', delete=False)
        self.p = subprocess.Popen([fibc, 'run', '--live', path], stdout=self.out, stderr=self.err, cwd=root, env=env)
        self.sock = None
        for _ in range(1800):
            if self.p.poll() is not None: break
            for line in open(self.err.name).read().splitlines():
                if line.startswith('fibc: live at '): self.sock = line[len('fibc: live at '):]
            if self.sock and os.path.exists(self.sock) and 'app: started' in self.stdout(): break
            time.sleep(0.1)
    def stdout(self): return open(self.out.name).read()
    def stderr(self): return open(self.err.name).read()
    def alive(self): return self.p.poll() is None
    def wait(self, secs=60):
        try: return self.p.wait(secs)
        except subprocess.TimeoutExpired: self.p.kill(); return None

class Client:
    def __init__(self, path):
        self.s = socket.socket(socket.AF_UNIX); self.s.settimeout(600); self.s.connect(path); self.buf = b''
    def ask(self, **req):
        """the answer, or {} when the program is gone (freed code ends it: the checks after say FAIL, not a traceback)"""
        try:
            self.s.sendall((json.dumps(req) + '\n').encode())
            while b'\n' not in self.buf:
                d = self.s.recv(1 << 20)
                if not d: return {}
                self.buf += d
            line, self.buf = self.buf.split(b'\n', 1)
            return json.loads(line)
        except OSError: return {}
    def eval(self, text): return self.ask(op='eval', text=text)
    def value(self, text): return self.eval(text).get('value')

def swapped(name, text, family, gen):
    s = c.eval(text)
    check(name, s.get('swapped') == [family] and s.get('generation') == gen, s)

def num(text):
    try: return int(c.value(text))
    except (TypeError, ValueError): return -1

a = App(os.path.join(here, 'retire-app.fib'))
check('started', a.sock is not None and a.alive(), 'stderr: %s stdout: %s' % (a.stderr()[-800:], a.stdout()))
if a.sock is None or not a.alive(): a.p.kill(); sys.exit(1)
c = Client(a.sock)

# ---- closures made by generation 1, kept in an atom, a def and a running thread; their families swapped again
swapped('make-adder swapped', '(defun make-adder (k: i64) -> (fn (i64) i64) (fn (x: i64) -> i64 (+ x (+ k 100))))', 'make-adder', 1)
check('atom holds a closure of generation 1', c.eval('(reset! adder (make-adder 2))').get('ok') and c.value('(@adder 1)') == '103')
check('def holds a closure of generation 1', c.eval('(def held: (fn (i64) i64) (make-adder 3))').get('ok') and c.value('(held 1)') == '104')
swapped('start-ticker swapped', '(defun start-ticker (by: i64) -> (Task i64) (spawn (fn () -> i64 (loop ((n 0)) (if @stop n (do (swap! ticks + (* by 10)) (live/pause-point) (u/sleep-ns 2000000) (recur (+ n 1))))))))', 'start-ticker', 2)
check('thread runs a closure of generation 2', c.eval('(def tk: (Task i64) (start-ticker 1))').get('ok'))
swapped('make-adder swapped again', '(defun make-adder (k: i64) -> (fn (i64) i64) (fn (x: i64) -> i64 (+ x (+ k 200))))', 'make-adder', 3)
swapped('start-ticker swapped again', '(defun start-ticker (by: i64) -> (Task i64) (spawn (fn () -> i64 (loop ((n 0)) (if @stop n (do (swap! ticks + by) (live/pause-point) (u/sleep-ns 2000000) (recur (+ n 1))))))))', 'start-ticker', 4)

# ---- a family that holds nothing: its old generation is freed
swapped('bump swapped', '(defun bump (n: i64) -> i64 (+ n 2))', 'bump', 5)
swapped('bump swapped again', '(defun bump (n: i64) -> i64 (+ n 3))', 'bump', 6)

# ---- the worker inside generation 7 of `step` (it reaches its pause point through `hook`), parked there, `step` swapped under it
swapped('step swapped', '(defun step (n: i64) -> i64 (do (@hook) (+ n 2)))', 'step', 7)
time.sleep(0.5)
r0 = num('@rounds')
check('worker runs', r0 > 0, r0)
p = c.ask(op='pause', timeout_ms=10000)
t = p.get('threads', {})
check('pause is quiescent', p.get('paused') is True and t.get('quiescent') is True and t.get('paused') == t.get('spawned', -1) + 1, p)
swapped('step swapped while parked', '(defun step (n: i64) -> i64 (do (@hook) (+ n 1)))', 'step', 8)

# ---- one retirement at quiescence
r = c.ask(op='retire')
print('  retire: %s' % json.dumps(r))
pinned = dict((x.get('function', '')[2:], x.get('reason')) for x in r.get('pinned', []))
check('retire answered', r.get('ok') is True, r)
check('step pinned: a thread is parked in it', 'step' in pinned, r)
check('make-adder and start-ticker pinned: closures of theirs are held', 'make-adder' in pinned and 'start-ticker' in pinned, r)
check('bump\'s old generation freed, bump not pinned', r.get('removed') == 1 and 'bump' not in pinned, r)
check('resumed', c.ask(op='resume').get('resumed') is True)
time.sleep(0.5)

# ---- after the resume: every held thing still runs
check('app alive after the resume', a.alive(), a.stderr()[-800:])
r1 = num('@rounds'); time.sleep(0.3); r2 = num('@rounds')
check('the worker returned out of the retired step and goes on', r1 > r0 and r2 > r1, (r0, r1, r2))
check('the atom\'s closure answers as generation 1 made it', c.value('(@adder 1)') == '103')
check('the def\'s closure answers as generation 1 made it', c.value('(held 1)') == '104')
k1 = num('@ticks'); time.sleep(0.3); k2 = num('@ticks')
check('the thread of generation 2 counts on', k1 > 0 and k2 > k1, (k1, k2))
check('a fresh closure is the current generation\'s', c.value('((make-adder 4) 1)') == '205')
check('bump answers from its current generation', c.value('(bump 1)') == '4')
check('retire again frees nothing held', c.ask(op='pause', timeout_ms=10000).get('paused') is True and c.ask(op='retire').get('ok') is True and c.ask(op='resume').get('resumed') is True and a.alive())

# ---- the end of the program
check('stop', c.eval('(reset! stop true)').get('ok'))
code = a.wait(60)
out = a.stdout().splitlines()
check('program ended', code == 0 and len(out) > 1 and re.fullmatch(r'app: stopped after \d+ rounds', out[-2]) and out[-1] == '0', (code, a.stdout()[-300:], a.stderr()[-300:]))
for f in (a.out.name, a.err.name):
    try: os.unlink(f)
    except OSError: pass
shutil.rmtree(runtime, ignore_errors=True)
print('live retire: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
