#!/usr/bin/env python3
"""compiler/tests/live/swap.py: the checks of body swaps in a running program (docs/design/live-repl.md 3.1 to 3.4, 5, package LV2;
serve.liveswap, serve.livetable, serve.liverules, emit.live) over a real socket against swap-app.fib, an HTTP app whose server holds
`handle` as a value made before any swap, with a worker thread calling `work` in a loop.
usage: swap.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested: transcripts 1, 4 and 5 of the design through `fibc repl --connect` (exact but for pids and timings; the generation
numbers are this sequence's); a swapped function called through a closure value made before the swap (the server's handler, a
function kept in an atom) runs the new body; a method swap; a generic function with two instantiations swaps both; 200 swaps of
`work` between two bodies while the worker calls it (every answer is one of the two; the app lives; `status` counts the generations);
a new function swaps nothing; the text refusals (live/Def, live/Layout, live/Protocol, live/Main) and the checker's; a trap after a
swap is still caught; the end of the program through the socket.
The mutants of scripts/mutant-live.sh that must make this fail: patch-before-jit, old-tracker-removed (the 200 swaps crash the app),
summary-ignored and signature-ignored (transcript 5's refusals do not come, and the accepted change breaks the app)."""
import json, os, re, shutil, socket, subprocess, sys, tempfile, time, urllib.request, urllib.error

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
app = os.path.join(here, 'swap-app.fib')
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

def free_port():
    s = socket.socket(); s.bind(('127.0.0.1', 0)); p = s.getsockname()[1]; s.close(); return p

class App:
    def __init__(self, path, args=()):
        self.out = tempfile.NamedTemporaryFile(prefix='live-out-', delete=False)
        self.err = tempfile.NamedTemporaryFile(prefix='live-err-', delete=False)
        self.p = subprocess.Popen([fibc, 'run', '--live', path] + list(args), stdout=self.out, stderr=self.err, cwd=root, env=env, preexec_fn=limit)
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
    def value(self, text): return self.eval(text).get('value')
    def close(self): self.s.close()

def get(port, path):
    """status and body of GET path"""
    try:
        with urllib.request.urlopen('http://127.0.0.1:%d%s' % (port, path), timeout=20) as r: return r.status, r.read().decode()
    except urllib.error.HTTPError as e: return e.code, e.read().decode()
    except Exception as e: return 0, str(e)

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

port = free_port()
a = App(app, ['--', '--port', str(port)])
check('started', a.sock is not None and a.alive() and 'app: listening on %d' % port in a.stdout(), 'stderr: %s stdout: %s' % (a.stderr()[-800:], a.stdout()))
if a.sock is None or not a.alive(): a.p.kill(); sys.exit(1)
c = Client(a.sock)
h = c.ask(op='hello')
check('hello generation 0', h.get('generation') == 0 and h.get('catching') is True, h)
check('before: root', get(port, '/?name=tom') == (200, 'root tom'))

# ---- transcript 1: edit a handler in a running server (the server holds `handle` as a closure value made at start-up)
out, r = terminal(a.sock, ['(defun handle (req: Request) -> Response (text-response 200 (str "hello, " (query req "name"))))'])
matches('transcript 1', out, [r'; attached to .*swap-app\.fib \(pid %d, generation 0, catching\)' % a.p.pid,
                              r'; swapped handle \(generation 1\): check \d+ ms, emit \d+ ms, jit \d+ ms'])
check('transcript 1: curl', get(port, '/?name=tom') == (200, 'hello, tom'), get(port, '/?name=tom'))
check('transcript 1: every path', get(port, '/items/7') == (200, 'hello, '), get(port, '/items/7'))
s = c.eval('(defun handle (req: Request) -> Response (route req))')
check('handle routes again', s.get('swapped') == ['handle'] and s.get('generation') == 2 and get(port, '/?name=tom') == (200, 'root tom'), s)

# ---- transcript 4: fix a bug found by a failing request
check('before the fix: /items/7 answers item 1', get(port, '/items/7') == (200, 'apple'), get(port, '/items/7'))
out, r = terminal(a.sock, ['(defun item-of (path: str) -> (Option Item) (get @items (parse-id path)))', '(item-of "/items/7")'])
matches('transcript 4', out, [r'; attached to .*', r'; swapped item-of \(generation 3\): check \d+ ms, emit \d+ ms, jit \d+ ms', r'nil'])
check('transcript 4: curl 404', get(port, '/items/7')[0] == 404, get(port, '/items/7'))
check('transcript 4: curl 200', get(port, '/items/2') == (200, 'pear'), get(port, '/items/2'))

# ---- transcript 5: a refused change, with a good error; nothing changes. The design's first line, `(defun handle (req: Request db: Db)
# ..)`, is refused by the checker here (`main` gives `handle` to `run-server`, which fixes its type); the signature rule's own refusal
# needs a change every caller still accepts: `tag`, only ever `(str (tag))`, returning a `str`.
check('before: /tag', get(port, '/tag') == (200, '7'))
out, r = terminal(a.sock, ['(defun tag () -> str "seven")',
                           '(defun handle (req: Request) -> Response (do (reset! last (some req)) (route req)))',
                           '(defun handle (req: Request db: Db) -> Response (route db req))'])
matches('transcript 5', out, [r'; attached to .*',
                              r'<input>:1:1: tag now has type \(fn :send \(\) str\), was \(fn :send \(\) i64\); callers were compiled against the old one \[live/Signature\]',
                              r"  hint: :restart changes signatures; the session's 3 definitions are kept and replayed",
                              r'; refused; the session and the program are unchanged \(generation 3\)',
                              r"<input>:1:1: handle's parameter req was borrowed and is now owned \(stored\); callers pass it borrowed \[live/Summary\]",
                              r'  hint: store \(clone req\), or :restart',
                              r'; refused; the session and the program are unchanged \(generation 3\)',
                              r'<session>:\d+:\d+: cannot unify \(fn :send \(Request Db\) Response\) with \(fn :send \(Request\) Response\) \[type/Unify\] \(in the definition of main\)',
                              r'; refused; the session and the program are unchanged \(generation 3\)'])
check('transcript 5: unchanged', get(port, '/tag') == (200, '7') and get(port, '/?name=tom') == (200, 'root tom') and c.ask(op='status').get('generation') == 3)
rr = c.eval('(defun tag () -> str "seven")')
check('refusal json', rr.get('ok') is False and rr.get('code') == 'live/Signature' and rr.get('hint', '').startswith(':restart') and rr.get('generation') == 3
      and rr['diagnostics'][0]['code'] == 'live/Signature', rr)
rr = c.eval('(defun handle (req: Request) -> Response (do (reset! last (some req)) (route req)))')
check('summary refusal json', rr.get('ok') is False and rr.get('code') == 'live/Summary' and rr.get('hint') == 'store (clone req), or :restart', rr)

# ---- a function held as a value before the swap (an atom made at start-up holds `score`'s closure constant)
check('before: /score/3', get(port, '/score/3') == (200, '30'))
s = c.eval('(defun score (n: i64) -> i64 (* n 100))')
check('swap score', s.get('swapped') == ['score'] and s.get('generation') == 4, s)
check('closure value runs the new body', get(port, '/score/3') == (200, '300'), get(port, '/score/3'))

# ---- a generic function: both instantiations swap
before = get(port, '/twice')
check('before: /twice', before[0] == 200 and before[1].startswith('42 '), before)
s = c.eval('(defun twice (x: a) :where ((Num a)) -> a (+ x (+ x x)))')
check('swap twice', s.get('swapped') == ['twice'] and s.get('generation') == 5, s)
after = get(port, '/twice')
check('both instantiations swapped', after[0] == 200 and after[1].startswith('63 ') and after[1] != before[1] and after[1].endswith('4.5'), after)

# ---- a method swap
check('before: /greet/1', get(port, '/greet/1') == (200, 'hi apple'))
s = c.eval('(impl Greet Item (greet (self) (str "hey " (. self name))))')
check('swap method', s.get('ok') and any(x.startswith('m.') and 'greet' in x for x in s.get('swapped', [])) and s.get('generation') == 6, s)
check('method runs the new body', get(port, '/greet/1') == (200, 'hey apple'), get(port, '/greet/1'))

# ---- a new function swaps nothing; definitions and expressions still work; a trap is still caught
d = c.eval('(defun thrice (x: i64) -> i64 (* 3 x))')
check('new function', d.get('ok') and d.get('defined') == ['thrice'] and not d.get('swapped') and d.get('generation') == 6, d)
check('new function runs', c.value('(thrice 7)') == '21')
s = c.eval('(defun thrice (x: i64) -> i64 (* 30 x))')
check('new function swapped', s.get('swapped') == ['thrice'] and s.get('generation') == 7 and c.value('(thrice 7)') == '210', s)
check('swapped function in a def', (c.eval('(def t3: (fn (i64) i64) thrice)') or {}).get('ok') and c.value('(t3 2)') == '60'
      and c.eval('(defun thrice (x: i64) -> i64 (* 300 x))').get('swapped') == ['thrice'] and c.value('(t3 2)') == '600')
t = c.eval('(nth [1 2] 5)')
check('trap still caught', t.get('ok') is False and 'index out of range' in t.get('failed', ''), t)
check('alive after trap', get(port, '/?name=x') == (200, 'root x'))
i = c.ask(op='inspect', name='thrice')
check('inspect a function', i.get('generation') == 8 and 'f.thrice.g8' in i.get('bodies', []) and 'f.thrice.owned.g8' in i.get('bodies', []), i)

# ---- the text refusals, each unchanged
for name, text, code in [('def', '(def items: (Atom (Map i64 Item)) (atom {}))', 'live/Def'),
                         ('type', '(defrecord Item (id: i64 name: str kind: str))', 'live/Layout'),
                         ('protocol', '(defprotocol Greet (greet (self) -> str) (wave (self) -> str))', 'live/Protocol'),
                         ('main', '(defun main () -> i64 0)', 'live/Main')]:
    r = c.eval(text)
    check('refused ' + name, r.get('ok') is False and r.get('code') == code and r.get('hint') == ':restart' and r.get('generation') == 8, r)
r = c.eval('(defun score (n: i64) -> i64 (+ n "a"))')
check('refused by the checker', r.get('ok') is False and r.get('shown') and 'type/' in r['diagnostics'][0]['code'] and get(port, '/score/3') == (200, '300'), r)
check('unchanged after refusals', c.ask(op='status').get('generation') == 8 and get(port, '/items/2') == (200, 'pear'))

# ---- 200 swaps of `work` while the worker calls it: every answer is one of the two bodies; the app lives; status counts them
st, calls0 = get(port, '/calls')
check('worker running', st == 200 and int(calls0.split()[0]) > 0, calls0)
gen = 8
bodies = ['(defun work (n: i64) -> i64 (loop ((i 0) (acc n)) (if (< i 4000) (recur (+ i 1) (+ acc (rem (* i 7) 13))) (- (rem acc 1) -2))))',
          '(defun work (n: i64) -> i64 (loop ((i 0) (acc n)) (if (< i 4000) (recur (+ i 1) (+ acc (rem (* i 11) 17))) (- (rem acc 1) -1))))']
swapped = 0
for n in range(200):
    s = c.eval(bodies[n % 2])
    if s.get('swapped') != ['work'] or s.get('generation') != gen + 1:
        check('swap %d' % n, False, s); break
    gen += 1; swapped += 1
check('200 swaps answered', swapped == 200, swapped)
st, calls1 = get(port, '/calls')
check('every answer is one of the two bodies', st == 200 and calls1.split()[1] == '0' and int(calls1.split()[0]) > int(calls0.split()[0]), (calls0, calls1))
check('app alive after the swaps', a.alive() and c.ask(op='hello').get('pid') == a.p.pid)
s = c.ask(op='status')
check('status counts the generations', s.get('generation') == gen and s['generations']['count'] == gen + 1 and s['generations']['functions'] > 10, s)
i = c.ask(op='inspect', name='work')
check('inspect work', i.get('generation') == gen and 'f.work.g%d' % gen in i.get('bodies', []), i)

# ---- the end of the program through the socket
check('stop', c.eval('(reset! stop true)').get('ok'))
code = a.wait(60)
out = a.stdout().splitlines()
check('program ended', code == 0 and out[0].startswith('app: listening') and out[-2].startswith('app: stopped after ') and out[-2].endswith('bad 0') and out[-1] == '0', (code, a.stdout()[-300:], a.stderr()[-300:]))
a.clean()
shutil.rmtree(runtime, ignore_errors=True)
print('live swap: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
