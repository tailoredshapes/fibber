#!/usr/bin/env python3
"""compiler/tests/live/stress.py: swaps under load, the pause and the retirement (docs/design/live-repl.md 3.3, 5 transcript 7,
package LV3; serve.liveretire, serve.livesplit, rt/live.lir, fib.os.live) over a real socket against stress-app.fib, then the http
pause point against swap-app.fib.
usage: stress.py FIBC [ROOT] [--swaps N]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested, against stress-app.fib (8 spawned workers and a driver of pool tasks call `f` in a loop): hello and status report the
threads and the pool; a thread inside a long call (`slow`, 1.5 s asleep, in a generation of its own) during a swap of it: the swap
does not wait, a retire meanwhile refuses, the old body finishes, the next call runs the new one; a trap in a spawned worker task and
in a pool task, each asked with try-join: the message
comes back, the program lives; N swaps (1000) of `f` between two bodies: every answer of the workers and of the pool tasks is one of
the two, the program lives, status counts the generations; `retire` while the stress runs refuses (`not-quiescent`) and frees
nothing; `pause` parks every thread (paused = spawned + main, the pool idle); `retire` then removes every generation but the slots'
current ones (status: 1 + the current generations of `f` and `slow`; `f` still answers), nothing pinned; `resume`, and the stress runs
right afterwards (counts grow, nothing bad) and swaps again; transcript 7 of the design through the terminal client (:status,
:retire refused, :pause, :retire, :resume) in its lines' shape; the end of the program through the socket, its counts in its output.
Then swap-app.fib: its http server's accept loop and a kept-alive connection's wait between requests are pause points: pause is
quiescent with a connection open, retire after a swap of `handle` removes the old generation, and a request on the kept connection
and a new one are answered after the resume.
The mutants of scripts/mutant-live.sh that must make this fail: retire-without-pause (the refusals do not come, and the retire
during the long call frees the generation the sleeping thread is in: the program dies when it wakes)."""
import json, os, re, shutil, socket, subprocess, sys, tempfile, time, urllib.request, urllib.error

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 and not sys.argv[2].startswith('--') else os.getcwd()
swaps = 1000
if '--swaps' in sys.argv: swaps = int(sys.argv[sys.argv.index('--swaps') + 1])
here = os.path.join(root, 'compiler', 'tests', 'live')
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
    def __init__(self, path, args=(), ready='app: started'):
        self.out = tempfile.NamedTemporaryFile(prefix='live-out-', delete=False)
        self.err = tempfile.NamedTemporaryFile(prefix='live-err-', delete=False)
        self.p = subprocess.Popen([fibc, 'run', '--live', path] + list(args), stdout=self.out, stderr=self.err, cwd=root, env=env, preexec_fn=limit)
        self.sock = None
        for _ in range(1800):
            if self.p.poll() is not None: break
            for line in open(self.err.name).read().splitlines():
                if line.startswith('fibc: live at '): self.sock = line[len('fibc: live at '):]
            if self.sock and os.path.exists(self.sock) and ready in self.stdout(): break
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
        """the answer, or {} when the program is gone (a mutant that frees live code ends it: the checks after say FAIL, not a traceback)"""
        try:
            self.s.sendall((json.dumps(req) + '\n').encode())
            line = self.line()
        except OSError: return {}
        return json.loads(line) if line else {}
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

def counts():
    """calls, bad, pool-calls, pool-bad, batches of the app"""
    v = c.value('[@calls @bad @pool-calls @pool-bad @batches]') or ''
    try: return [int(x) for x in v.strip('[]').split()]
    except ValueError: return [-1] * 5

bodies = ['(defun f (n: i64) -> i64 (loop ((i 0) (acc n)) (if (< i 1000) (recur (+ i 1) (+ acc (rem (* i 7) 13))) (- (rem acc 1) -2))))',
          '(defun f (n: i64) -> i64 (loop ((i 0) (acc n)) (if (< i 1000) (recur (+ i 1) (+ acc (rem (* i 11) 17))) (- (rem acc 1) -1))))']

# ---- the stress app
a = App(os.path.join(here, 'stress-app.fib'))
check('started', a.sock is not None and a.alive() and 'app: started' in a.stdout(), 'stderr: %s stdout: %s' % (a.stderr()[-800:], a.stdout()))
if a.sock is None or not a.alive(): a.p.kill(); sys.exit(1)
c = Client(a.sock)
h = c.ask(op='hello')
check('hello threads and pool', h.get('generation') == 0 and h.get('threads') == 9 and h.get('pool', 0) >= 1, h)
time.sleep(1.0)
c0 = counts()
check('load running', c0[0] > 0 and c0[1] == 0 and c0[2] > 0 and c0[3] == 0 and c0[4] > 0, c0)
s = c.ask(op='status')
t = s.get('threads', {})
check('status threads', t.get('spawned') == 9 and t.get('pausing') is False and t.get('quiescent') is False and t.get('pool_workers', 0) >= 1 and t.get('paused') == 0, s)

# ---- a thread inside a long call during a swap of it: the swap does not wait; a retire meanwhile refuses (the thread is in the old
# generation, which is a tracker of its own once `slow` was swapped before the call: the mutant retire-without-pause frees it under
# the sleeping thread, which then returns into unmapped code); the old body finishes, the next call runs the new one
s = c.eval('(defun slow (n: i64) -> i64 (do (u/sleep-ns 1500000000) (+ n 1000)))')
check('long call swapped once', s.get('swapped') == ['slow'] and s.get('generation') == 1, s)
d = c.eval('(def slow-task: (Task i64) (future (slow 1)))')
check('long call started', d.get('ok') and d.get('defined') == ['slow-task'], d)
t0 = time.time()
s = c.eval('(defun slow (n: i64) -> i64 (do (u/sleep-ns 1500000000) (+ n 2000)))')
dt = time.time() - t0
check('swap during the long call does not wait', s.get('swapped') == ['slow'] and s.get('generation') == 2 and dt < 1.2, (s, dt))
r = c.ask(op='retire')
check('retire during the long call refuses', r.get('ok') is False and r.get('error', {}).get('code') == 'not-quiescent', r)
check('the old body finishes', c.value('@slow-task') == '1001' and a.alive())
check('the next call runs the new body', c.value('(slow 1)') == '2001')

# ---- a trap in a worker task and in a pool task: isolated by try-join, the program lives
r = c.value('(match (try-join (spawn (fn () -> i64 (boom 5)))) ((Ok v) (str "ok " v)) ((Err e) (str "trap: " (. e message))))')
check('trap in a spawned worker task', r == '"trap: nth: index out of range"' and a.alive(), r)
r = c.value('(match (try-join (fork-task (fn () -> i64 (boom 5)))) ((Ok v) (str "ok " v)) ((Err e) (str "trap: " (. e message))))')
check('trap in a pool task', r == '"trap: nth: index out of range"' and a.alive(), r)
check('load still running after the traps', counts()[0] > c0[0] and counts()[1] == 0 and a.alive())

# ---- the swaps under load
gen = 2; done = 0; t0 = time.time()
for n in range(swaps):
    s = c.eval(bodies[n % 2])
    if s.get('swapped') != ['f'] or s.get('generation') != gen + 1:
        check('swap %d' % n, False, s); break
    gen += 1; done += 1
dt = time.time() - t0
check('%d swaps answered' % swaps, done == swaps, done)
print('  %d swaps in %.1f s, %.0f ms each' % (done, dt, 1000 * dt / max(done, 1)))
c1 = counts()
check('every answer of the workers is one of the two bodies', c1[0] > c0[0] and c1[1] == 0, (c0, c1))
check('every answer of the pool tasks is one of the two bodies', c1[2] > c0[2] and c1[3] == 0 and c1[4] > c0[4], (c0, c1))
check('app alive after the swaps', a.alive() and c.ask(op='hello').get('pid') == a.p.pid)
s = c.ask(op='status')
check('status counts the generations', s.get('generation') == gen and s['generations']['count'] == gen + 1, s)

# ---- retire while the stress runs: refused, nothing freed
r = c.ask(op='retire')
check('retire while not quiescent refuses', r.get('ok') is False and r.get('error', {}).get('code') == 'not-quiescent' and 'pause first' in r['error']['message'], r)
check('nothing freed by the refusal', c.ask(op='status')['generations']['count'] == gen + 1 and a.alive())
c2 = counts()
check('load runs on', c2[0] > c1[0] and c2[1] == 0 and c2[3] == 0)

# ---- pause: every thread parked, the pool idle
p = c.ask(op='pause', timeout_ms=10000)
t = p.get('threads', {})
check('pause is quiescent', p.get('ok') and p.get('paused') is True and t.get('quiescent') is True and t.get('pausing') is True, p)
check('every thread parked', t.get('paused') == t.get('spawned', -1) + 1 and t.get('spawned') == 9 and t.get('pool_active') == 0 and t.get('async_busy') == 0, t)
c3 = counts()
time.sleep(0.5)
check('nothing runs while paused', counts() == c3, (c3, counts()))
check('status while paused', c.ask(op='status')['threads']['quiescent'] is True)
check('a swap while paused', c.eval(bodies[gen % 2]).get('generation') == gen + 1)
gen += 1

# ---- retire at quiescence: only the slots' current generations stay
r = c.ask(op='retire')
check('retire removes the old generations', r.get('ok') and r.get('removed') == gen - 2 and r.get('kept') == 2 and r.get('pinned') == [] and r.get('generation') == gen, r)
s = c.ask(op='status')
check('status after retire: the slots\' generations only', s['generations']['count'] == 3 and s.get('generation') == gen, s)
i = c.ask(op='inspect', name='f')
check('f still at its generation', i.get('generation') == gen and 'f.f.g%d' % gen in i.get('bodies', []), i)
check('f still answers', c.value('(f 3)') in ('1', '2'))
check('retire again removes nothing', c.ask(op='retire').get('removed') == 0)

# ---- resume: the stress runs right afterwards
r = c.ask(op='resume')
check('resumed', r.get('ok') and r.get('resumed') is True, r)
time.sleep(1.0)
c4 = counts()
check('load runs after the resume', c4[0] > c3[0] and c4[2] > c3[2] and c4[1] == 0 and c4[3] == 0 and a.alive(), (c3, c4))
t = c.ask(op='status')['threads']
check('nothing parked after the resume', t.get('paused') == 0 and t.get('pausing') is False, t)
for n in range(20):
    s = c.eval(bodies[gen % 2])   # the other body: the one `f` has now is redefined with no change (LM2: nothing swapped)
    if s.get('swapped') != ['f']: check('swap after retire %d' % n, False, s); break
    gen += 1
time.sleep(0.5)
c5 = counts()
check('swaps after the retirement run right', c5[0] > c4[0] and c5[1] == 0 and c5[3] == 0 and a.alive(), (c4, c5))

# ---- transcript 7 of the design, in the terminal client: the lines' shape (the bytes of a generation are not reported, LV2 (f))
out, r = terminal(a.sock, [':status', ':retire', ':pause', ':retire', ':resume'])
matches('transcript 7', out, [r'; attached to .*stress-app\.fib \(pid %d, generation %d, catching\); 9 threads, pool \d+ workers' % (a.p.pid, gen),
                              r'; generation %d; generations kept \d+; modules kept \d+; threads: 9 spawned threads \(0 parked\) and \d+ pool workers \(\d+ active\); values: \d+ resident; inputs \d+; tests: idle' % gen,
                              r'; not quiescent: no pause was requested; pause first \[not-quiescent\]',
                              r'; paused at 10 pause points; pool idle \(\d+ workers\)',
                              r'; removed 20 generations; 2 kept \(generation %d\)' % gen,
                              r'; resumed; 9 spawned threads \(\d+ parked\) and \d+ pool workers \(\d+ active\)'])
time.sleep(0.3)
c6 = counts()
check('load runs after transcript 7', c6[0] > c5[0] and c6[1] == 0 and c6[3] == 0)

# ---- the end of the program through the socket
check('stop', c.eval('(reset! stop true)').get('ok'))
code = a.wait(60)
out = a.stdout().splitlines()
check('program ended', code == 0 and out[0] == 'app: started' and re.fullmatch(r'app: stopped after \d+ calls, bad 0; pool \d+ calls in \d+ rounds, bad 0', out[-2] if len(out) > 1 else '') and out[-1] == '0',
      (code, a.stdout()[-300:], a.stderr()[-300:]))
a.clean()

# ---- the http server's pause points (swap-app.fib: the accept loop, a kept-alive connection's wait between requests)
def free_port():
    s = socket.socket(); s.bind(('127.0.0.1', 0)); p = s.getsockname()[1]; s.close(); return p

def get(port, path):
    try:
        with urllib.request.urlopen('http://127.0.0.1:%d%s' % (port, path), timeout=20) as r: return r.status, r.read().decode()
    except urllib.error.HTTPError as e: return e.code, e.read().decode()
    except Exception as e: return 0, str(e)

def keepalive_get(s, port, path):
    s.sendall(('GET %s HTTP/1.1\r\nHost: 127.0.0.1:%d\r\n\r\n' % (path, port)).encode())
    data = b''
    while b'\r\n\r\n' not in data:
        d = s.recv(65536)
        if not d: return None
        data += d
    head, body = data.split(b'\r\n\r\n', 1)
    n = int(re.search(rb'Content-Length: (\d+)', head, re.I).group(1))
    while len(body) < n: body += s.recv(65536)
    return head.split(b'\r\n')[0].decode(), body.decode()

port = free_port()
b = App(os.path.join(here, 'swap-app.fib'), ['-I', here, '--', '--port', str(port)], ready='app: listening')
check('http app started', b.sock is not None and b.alive() and 'app: listening on %d' % port in b.stdout(), 'stderr: %s stdout: %s' % (b.stderr()[-800:], b.stdout()))
if b.sock is None or not b.alive(): b.p.kill(); sys.exit(1)
c = Client(b.sock)
check('http before', get(port, '/?name=tom') == (200, 'root tom'))
ka = socket.create_connection(('127.0.0.1', port), timeout=30)
check('kept-alive request', keepalive_get(ka, port, '/?name=ka') == ('HTTP/1.1 200 OK', 'root ka'))
ka_idle_from = time.monotonic()
s = c.eval('(defun handle (req: Request) -> Response (text-response 200 (str "hello, " (query req "name"))))')
check('http swap', s.get('swapped') == ['handle'] and s.get('generation') == 1, s)
p = c.ask(op='pause', timeout_ms=10000)
t = p.get('threads', {})
check('http pause with a connection open', p.get('paused') is True and t.get('quiescent') is True and t.get('spawned') == 3 and t.get('paused') == 4, p)
r = c.ask(op='retire')
check('http retire', r.get('ok') and r.get('removed') == 0 and r.get('kept') == 1 and r.get('pinned') == [], r)
s = c.eval('(defun handle (req: Request) -> Response (text-response 200 (str "hi, " (query req "name"))))')
r = c.ask(op='retire')
check('http retire after a second swap', s.get('generation') == 2 and r.get('ok') and r.get('removed') == 1 and r.get('kept') == 1, (s, r))
check('http resume', c.ask(op='resume').get('resumed') is True)
got = keepalive_get(ka, port, '/?name=ka2')
check('kept-alive request after the resume', got == ('HTTP/1.1 200 OK', 'hi, ka2'), (got, 'idle %.1f s' % (time.monotonic() - ka_idle_from)))
check('new request after the resume', get(port, '/?name=tom') == (200, 'hi, tom'), get(port, '/?name=tom'))
ka.close()
check('http stop', c.eval('(reset! stop true)').get('ok'))
code = b.wait(60)
out = b.stdout().splitlines()
check('http program ended', code == 0 and out[-2].startswith('app: stopped after ') and out[-2].endswith('bad 0') and out[-1] == '0', (code, b.stdout()[-300:], b.stderr()[-300:]))
b.clean()
shutil.rmtree(runtime, ignore_errors=True)
print('live stress: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
