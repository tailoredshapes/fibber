#!/usr/bin/env python3
"""compiler/tests/live/defs.py: `def`s redefined in a live session (package LT4 of docs/design/live-meshql.md, docs/design/live-repl.md 9;
serve.livedefs, emit.program `live-def-slot`, emit.compile `request-def`) over a real socket against defs-app.fib (as app.fib in a scratch
directory) with defs-cfg.fib as src/cfg.fib (a -I session), edited on disk between loads.
usage: defs.py FIBC [ROOT] [--redefs N]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested: a `def` of the same type redefined at the REPL is seen by a resident function on its next read (an i64, a vector);
a `def` whose type changes is live/Def and nothing changes; a `def` of no slot (an Option, held inline) is live/Def; `defonce` keeps an
atom's value and a plain `def` makes it again; a load of cfg with `base` changed answers the new value from a resident function, a new
`def` with a side effect in its initialiser is made once (and read by a new function of the file), its redefinition runs the new
initialiser once; status counts the redefinitions; under the stress (8 threads reading `limit` and `table` while both are redefined N
times, default 1000) every read is the old value or the new one and the program lives; pause and retire drop the old vectors (status:
dropped, nothing retiring) and the reads go on after the resume; the program ends through the socket with no bad read.
The mutants of scripts/mutant-live.sh that must make this fail: old-def-freed (the stress), init-run-twice (the new def's side effect)."""
import json, os, re, shutil, socket, subprocess, sys, tempfile, time

args = [a for a in sys.argv[1:] if not a.startswith('--')]
fibc = os.path.abspath(args[0])
root = os.path.abspath(args[1]) if len(args) > 1 else os.getcwd()
redefs = int(sys.argv[sys.argv.index('--redefs') + 1]) if '--redefs' in sys.argv else 1000
here = os.path.join(root, 'compiler', 'tests', 'live')
work = tempfile.mkdtemp(prefix='live-defs-', dir=os.environ.get('TMPDIR') or None)
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')   # a unix socket path is at most 100 bytes (serve.sock)
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')))
env.pop('FIB_LIVE', None)
failed = []
os.mkdir(os.path.join(work, 'src'))
shutil.copy(os.path.join(here, 'defs-app.fib'), os.path.join(work, 'app.fib'))
cfg0 = open(os.path.join(here, 'defs-cfg.fib')).read()
cfg_file = os.path.join(work, 'src', 'cfg.fib')

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
        self.p = subprocess.Popen([fibc, 'run', '--live'] + list(args) + [path], stdout=self.out, stderr=self.err, cwd=work, env=env, preexec_fn=limit)
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
    def value(self, text): return self.eval(text).get('value')
    def load(self, file): return self.ask(op='load', file=file) or {}

def shown(a): return ' '.join(a.get('shown', []))

with open(cfg_file, 'w') as f: f.write(cfg0)
a = App('app.fib', ['-I', 'src'])
check('started', a.sock is not None and a.alive() and 'app: ready 10' in a.stdout(), 'stderr: %s stdout: %s' % (a.stderr()[-800:], a.stdout()))
if a.sock is None or not a.alive(): a.p.kill(); sys.exit(1)
c = Client(a.sock)

# ---- the REPL: same type, type change, no slot, defonce
check('before', c.value('(read-limit)') == '3' and c.value('(table-sum)') == '768', c.value('(read-limit)'))
r = c.eval('(def limit: i64 5)')
check('limit redefined', r.get('ok') is True and c.value('(read-limit)') == '5' and c.value('limit') == '5', r)
r = c.eval('(def table: (Vec i64) (vec (repeat 256 5)))')
check('table redefined: the resident function reads the new vector', r.get('ok') is True and c.value('(table-sum)') == '1280', (r, c.value('(table-sum)')))
r = c.eval('(def label: str "one")')
check('a type change is live/Def', r.get('ok') is False and r.get('code') == 'live/Def' and 'i64' in shown(r) and 'str' in shown(r) and c.value('(show-label)') == '"1"', r)
r = c.eval('(def maybe: (Option i64) (some 2))')
check('a value held inline is live/Def', r.get('ok') is False and r.get('code') == 'live/Def' and c.value('(maybe-value)') == '1', r)
c.eval('(swap! counter (fn (n: i64) (+ n 1)))')
r = c.eval('(defonce counter: (Atom i64) (atom 0))')
check('defonce keeps the value', r.get('ok') is True and c.value('@counter') == '1', (r, c.value('@counter')))
r = c.eval('(def counter: (Atom i64) (atom 7))')
check('def makes it again', r.get('ok') is True and c.value('@counter') == '7', (r, c.value('@counter')))

# ---- load: a changed def, a new def with a side effect, its redefinition
with open(cfg_file, 'w') as f: f.write(cfg0.replace('(def base: i64 10)', '(def base: i64 20)'))
r = c.load('src/cfg.fib')
check('load: a changed def', r.get('ok') is True and c.value('(c/base-value)') == '20', r)
new = cfg0.replace('(def base: i64 10)', '(def base: i64 20)') + '(def stamp: i64 (do (swap! inits (fn (n: i64) (+ n 1))) 42))\n(defun stamp-value () -> i64 stamp)\n'
with open(cfg_file, 'w') as f: f.write(new)
r = c.load('src/cfg.fib')
check('load: a new def, its initialiser run once', r.get('ok') is True and c.value('(c/stamp-value)') == '42' and c.value('@c/inits') == '1', (r, c.value('@c/inits')))
with open(cfg_file, 'w') as f: f.write(new.replace('42))', '43))'))
r = c.load('src/cfg.fib')
check('load: the new def redefined, once', r.get('ok') is True and c.value('(c/stamp-value)') == '43' and c.value('@c/inits') == '2', (r, c.value('@c/inits')))
with open(cfg_file, 'w') as f: f.write(new.replace('(def base: i64 10)', '(def base: str "x")').replace('(def base: i64 20)', '(def base: str "x")').replace('(defun base-value () -> i64 base)', '(defun base-value () -> i64 0)'))
r = c.load('src/cfg.fib')
check('load: a type change is live/Def at the definition', r.get('ok') is False and r.get('code') == 'live/Def' and re.search(r'src/cfg\.fib:4:1: base is a def of type i64', shown(r)) and c.value('(c/base-value)') == '20', r)
st = c.ask(op='status')
check('status counts the redefinitions', st.get('defs', {}).get('redefined') == 5, st.get('defs'))

# ---- the stress: both redefined `redefs` times while 8 threads read them
t0 = time.time(); bad_answers = 0
for i in range(redefs):
    v = 3 if i % 2 == 0 else 5
    r = c.eval('(def limit: i64 %d)\n(def table: (Vec i64) (vec (repeat 256 (+ %d @zero))))' % (v, v))
    if r.get('ok') is not True: bad_answers += 1
    if not a.alive(): break
check('stress: every redefinition accepted', bad_answers == 0 and a.alive(), (bad_answers, a.stderr()[-600:]))
check('stress: every read the old value or the new', c.value('@bad') == '0', c.value('@bad'))
st = c.ask(op='status')
print('stress: %d redefinitions in %.1f s; defs %s' % (redefs, time.time() - t0, st.get('defs')))
check('stress: the old vectors wait for a retirement', st['defs']['retiring'] >= redefs and st['defs']['dropped'] == 0, st.get('defs'))

# ---- pause, retire: the old values dropped
p = c.ask(op='pause', timeout_ms=10000)
check('pause: quiescent', p.get('ok') and p['threads']['quiescent'] is True, p)
r = c.ask(op='retire')
d = r.get('defs', {})
check('retire: the old vectors dropped', r.get('ok') is True and d.get('retiring') == 0 and d.get('dropped', 0) >= redefs - 1 and d.get('dropped', 0) + d.get('immortal', 0) >= redefs + 1, r)
c.ask(op='resume')
time.sleep(0.5)
check('after the retirement: reads go on', c.value('@bad') == '0' and c.value('(table-sum)') in ('768', '1280') and a.alive(), c.value('@bad'))

# ---- the end
check('stop', c.eval('(reset! stop true)').get('ok'))
code = a.wait(60)
out = a.stdout().splitlines()
check('program ended with no bad read', code == 0 and 'app: stopped, bad 0' in out, (code, a.stdout()[-300:], a.stderr()[-300:]))
a.clean()
shutil.rmtree(runtime, ignore_errors=True)
shutil.rmtree(work, ignore_errors=True)
print('live defs: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
