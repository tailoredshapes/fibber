#!/usr/bin/env python3
"""compiler/tests/live/load.py: the checks of `load` and `reload` of a live session (docs/design/live-incremental.md 3, package LM1;
serve.liveload, serve.incr `Job.texts`, expand.modules `try-load-texts`, emit.compile `BodyRequest.module`, serve.liveclient `:load`)
over a real socket against load-app.fib (as app.fib in a scratch directory, the session's working directory) with load-routes.fib as
src/routes.fib (a -I session), edited on disk between loads.
usage: load.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested: an edit of `route` loaded: `swapped: ["routes/route"]`, the session's next call and the app's loop (a resident caller)
answer the edit; a bad edit is refused with a diagnostic at src/routes.fib:L:C and nothing changes; a file with one refused definition
(an arity change of `route`, which app.fib calls; a def whose type changes under a reader, `route`, that would not check: live/Def
naming it, LT4) applies nothing: the good edit of `greet` in the same file is
not swapped and the answer lists the refusal; a removed function is live/Removed and still callable; `reload` after a second edit swaps
again and `reload` with no change swaps nothing; an edit of a comment only swaps nothing (LM2), of a def's comment redefines nothing; a half-saved file on disk is not seen by an expression input nor `:type` (the override), `status`
says the file is stale; a new function in the loaded file is defined and then swapped; a file whose module the program lacks is
live/NotLoaded with the `require` hint; a load of the main module's file defines its new function; the terminal client's `:load`,
`:status` and `:reload` lines; the app ends through the socket.
The mutants of scripts/mutant-live.sh that must make this fail: partial-load, load-reads-disk, prefix-dropped, removed-ignored,
rules-at-file-start, type-reads-disk."""
import json, os, re, shutil, socket, subprocess, sys, tempfile, time

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
work = tempfile.mkdtemp(prefix='live-load-', dir=os.environ.get('TMPDIR') or None)   # the session's working directory: app.fib, src/routes.fib
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')   # a unix socket path is at most 100 bytes (serve.sock)
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')))
env.pop('FIB_LIVE', None)
failed = []
os.mkdir(os.path.join(work, 'src'))
shutil.copy(os.path.join(here, 'load-app.fib'), os.path.join(work, 'app.fib'))
routes0 = open(os.path.join(here, 'load-routes.fib')).read()
routes_file = os.path.join(work, 'src', 'routes.fib')

def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)

def limit():
    if sys.platform != 'linux': return
    import resource
    try: resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
    except (ValueError, OSError): pass

def write_routes(text):
    with open(routes_file, 'w') as f: f.write(text)

def edited(old, new, base=None):
    """routes0 (or base) with one definition's text replaced"""
    text = base if base is not None else routes0
    assert old in text, old
    return text.replace(old, new)

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
    def close(self): self.s.close()

def terminal(sock, lines):
    """the terminal client's output lines for the inputs (the raw client is closed meanwhile: the program serves one connection at a time)"""
    global c
    c.close()
    r = subprocess.run([fibc, 'repl', '--connect', sock], input=''.join(l + '\n' for l in lines) + ':quit\n', capture_output=True, text=True, cwd=work, env=env, timeout=600, preexec_fn=limit)
    c = Client(sock)
    return r.stdout.splitlines(), r

def matches(name, got, want):
    """`want` is one regex (anchored) per expected line"""
    ok = len(got) == len(want) and all(re.fullmatch(w, g) for g, w in zip(got, want))
    check(name, ok, '\n  got:  %s\n  want: %s' % (got, want))

def shown_at(a, code):
    """the shown line of a refusal with `code`, or ''"""
    for l in a.get('shown', []):
        if l.endswith('[%s]' % code): return l
    return ''

write_routes(routes0)
a = App('app.fib', ['-I', 'src'])
check('started', a.sock is not None and a.alive() and 'app: ready' in a.stdout(), 'stderr: %s stdout: %s' % (a.stderr()[-800:], a.stdout()))
if a.sock is None or not a.alive(): a.p.kill(); sys.exit(1)
c = Client(a.sock)
h = c.ask(op='hello')
check('hello generation 0', h.get('generation') == 0, h)
time.sleep(0.2)
check('before: the app calls route', c.value('@last') == '"route tick"' and c.value('(r/route "x")') == '"route x"', c.value('@last'))
st = c.ask(op='status')
check('status before any load: no loaded file, the name rule', st.get('loaded') == [] and st.get('live') == 'every module not named fib.*', st)

# ---- an edit of route loaded: swapped, the session's call and the app's loop answer the edit
write_routes(edited('(defun route (path: str) -> str (str prefix " " path))', '(defun route (path: str) -> str (str prefix "! " path))'))
l = c.load('src/routes.fib')
check('load: swapped routes/route', l.get('ok') is True and l.get('swapped') == ['routes/route'] and l.get('generation') == 1 and l.get('module') == 'routes'
      and l.get('file') == 'src/routes.fib' and l.get('definitions') == 4 and l.get('changed') == ['routes/route'] and l.get('defined') == []
      and l.get('swap', {}).get('check_ms', -1) >= 0, l)
check('load: the next call answers the edit', c.value('(r/route "x")') == '"route! x"', c.value('(r/route "x")'))
time.sleep(0.2)
check('load: the app\'s loop (a resident caller) answers the edit', c.value('@last') == '"route! tick"', c.value('@last'))
st = c.ask(op='status')
check('status: the loaded file, not stale', st.get('loaded') == [{'file': 'src/routes.fib', 'module': 'routes', 'stale': False}], st.get('loaded'))

# ---- a bad edit: a diagnostic at the file's position; nothing changes
write_routes(edited('(defun route (path: str) -> str (str prefix " " path))', '(defun route (path: str) -> str (+ path 1))'))
l = c.load('src/routes.fib')
check('a bad edit is refused at src/routes.fib:L:C', l.get('ok') is False and l.get('shown') and re.match(r'src/routes\.fib:\d+:\d+: .*\[type/\w+\]', l['shown'][0]) and l.get('generation') == 1
      and l['diagnostics'][0]['file'] == 'src/routes.fib', l)
check('a bad edit changes nothing', c.value('(r/route "x")') == '"route! x"' and c.ask(op='status').get('generation') == 1)

# ---- all or nothing: one refused definition in the file, the good one is not swapped
write_routes(edited('(defun greet (name: str) -> str (str "hello " name))', '(defun greet (name: str) -> str (str "hi " name))',
                    edited('(defun route (path: str) -> str (str prefix " " path))', '(defun route (path: str n: i64) -> str (str prefix " " path n))')))
l = c.load('src/routes.fib')
check('an arity change of a function app.fib calls is refused by the checker', l.get('ok') is False and l.get('shown') and re.match(r'<session>:\d+:\d+: route takes 2 argument\(s\), got 1 \[type/\w+\] \(in the definition of main\)', l['shown'][0]), l)
check('the good edit of the same file is not swapped', c.value('(r/greet "a")') == '"hello a"' and c.ask(op='status').get('generation') == 1, c.value('(r/greet "a")'))
write_routes(edited('(defun greet (name: str) -> str (str "hello " name))', '(defun greet (name: str) -> str (str "hi " name))',
                    edited('(def prefix: str "route")', '(def prefix: (fn () str) (fn () "route"))')))
l = c.load('src/routes.fib')
check('a def whose type changes under a reader that does not check is live/Def at its position', l.get('ok') is False and l.get('code') == 'live/Def'
      and re.match(r'src/routes\.fib:5:1: prefix changes type here, and its reader route would not type-check.*\[live/Def\]', shown_at(l, 'live/Def'))
      and l.get('hint') == ':restart' and l['diagnostics'][0]['line'] == 5, l)
check('nothing of the file was applied', c.value('(r/greet "a")') == '"hello a"' and c.value('(r/route "x")') == '"route! x"' and c.ask(op='status').get('generation') == 1, c.value('(r/greet "a")'))

# ---- a removed function is live/Removed (its stub stays); it is still callable
check('before: helper', c.value('(r/helper 1)') == '2')
write_routes(edited('(defun helper (n: i64) -> i64 (* n 2))\n', ''))
l = c.load('src/routes.fib')
check('a removed function is live/Removed', l.get('ok') is False and l.get('code') == 'live/Removed' and 'helper was removed' in shown_at(l, 'live/Removed'), l)
check('the removed function is still callable', c.value('(r/helper 1)') == '2' and c.ask(op='status').get('generation') == 1, c.value('(r/helper 1)'))

# ---- a function the session reached first (helper: the app never calls it; its stub is resident through the expression's module, and
# the live table lacks it) edited and loaded: swapped, not defined again
write_routes(edited('(defun helper (n: i64) -> i64 (* n 2))', '(defun helper (n: i64) -> i64 (* n 3))',
                    edited('(defun route (path: str) -> str (str prefix " " path))', '(defun route (path: str) -> str (str prefix "! " path))')))
l = c.load('src/routes.fib')
check('a function first reached from the REPL is swapped by a load, not defined again', l.get('ok') is True and l.get('swapped') == ['routes/helper'] and l.get('defined') == [] and l.get('generation') == 2, l)
check('the swapped function answers', c.value('(r/helper 1)') == '3', c.value('(r/helper 1)'))
write_routes(edited('(defun route (path: str) -> str (str prefix " " path))', '(defun route (path: str) -> str (str prefix "! " path))'))
l = c.load('src/routes.fib')
check('and back', l.get('ok') is True and l.get('swapped') == ['routes/helper'] and l.get('generation') == 3 and c.value('(r/helper 1)') == '2', l)

# ---- a rules refusal (live/Signature: helper is in the live table now) is reported at the definition's line, not 1:1
write_routes(edited('(defun helper (n: i64) -> i64 (* n 2))', '(defun helper (n: i64) -> str (str n))',
                    edited('(defun route (path: str) -> str (str prefix " " path))', '(defun route (path: str) -> str (str prefix "! " path))')))
l = c.load('src/routes.fib')
check('a signature refusal is at the definition in the file', l.get('ok') is False and l.get('code') == 'live/Signature'
      and re.match(r'src/routes\.fib:11:1: helper now has type .*\[live/Signature\]', shown_at(l, 'live/Signature'))
      and l['diagnostics'][0]['line'] == 11 and l['diagnostics'][0]['col'] == 1 and c.ask(op='status').get('generation') == 3, l)

# ---- reload after a second edit swaps again; reload with no change swaps nothing
write_routes(edited('(defun route (path: str) -> str (str prefix " " path))', '(defun route (path: str) -> str (str prefix "? " path))'))
st = c.ask(op='status')
check('status: the file is stale before the reload', st.get('loaded') == [{'file': 'src/routes.fib', 'module': 'routes', 'stale': True}], st.get('loaded'))
r = c.ask(op='reload')
check('reload swaps the second edit', r.get('ok') is True and len(r.get('loaded', [])) == 1 and r['loaded'][0].get('swapped') == ['routes/route'] and r['loaded'][0].get('file') == 'src/routes.fib'
      and r.get('generation') == 4, r)
check('the second edit answers', c.value('(r/route "x")') == '"route? x"', c.value('(r/route "x")'))
r = c.ask(op='reload')
check('reload with no change swaps nothing', r.get('ok') is True and r.get('loaded') == [] and r.get('generation') == 4, r)

# ---- change by expanded form (LM2): an edit of a comment only is loaded and swaps nothing
write_routes(edited('(defun route (path: str) -> str (str prefix " " path))', '(defun route (path: str) -> str\n  ;; a comment: the text changes, the expansion does not\n  (str prefix "? " path))'))
l = c.load('src/routes.fib')
check('a comment-only edit swaps nothing', l.get('ok') is True and not l.get('swapped') and l.get('changed') == [] and l.get('generation') == 4
      and c.value('(r/route "x")') == '"route? x"', l)
cur = open(routes_file).read()
write_routes(cur + '\n(def counter: (Atom i64) (atom 0))\n')
l = c.load('src/routes.fib')
check('a new def of a run-time value is made', l.get('ok') is True and c.value('(swap! r/counter inc)') == '1', l)
write_routes(cur + '\n(def counter: (Atom i64)\n  ;; a comment only: the expanded form is the same\n  (atom 0))\n')
l = c.load('src/routes.fib')
check('a comment-only edit of a def redefines nothing (by expanded form, as functions): its value is kept', l.get('ok') is True and not l.get('swapped')
      and l.get('generation') == 4 and c.value('@r/counter') == '1', (l, c.value('@r/counter')))
write_routes(cur + '\n(def counter: (Atom i64) (atom 10))\n')
l = c.load('src/routes.fib')
check('a def whose expanded form changed is redefined', l.get('ok') is True and c.value('@r/counter') == '10', (l, c.value('@r/counter')))

# ---- the override: a half-saved file on disk is not seen by an expression input; reload brings the disk in
write_routes(edited('(defun route (path: str) -> str (str prefix "? " path))', '(defun route (path: str) -> str (str prefix "?? " path', edited('(defun route (path: str) -> str (str prefix " " path))', '(defun route (path: str) -> str (str prefix "? " path))')))
v = c.eval('(r/route "x")')
check('an expression input runs the loaded text, not the half-saved disk file', v.get('value') == '"route? x"', v)
t = c.ask(op='type', expr='(r/route "x")')
check(':type reads the loaded text, not the half-saved disk file', t.get('ok') is True and t.get('type') == 'str', t)
st = c.ask(op='status')
check('status: stale while the disk differs', st.get('loaded') and st['loaded'][0]['stale'] is True, st.get('loaded'))
r = c.ask(op='reload')
check('reload of the half-saved file is a read error, nothing changes', r.get('ok') is False and r['loaded'][0].get('code') == 'read' and c.value('(r/route "x")') == '"route? x"', r)

# ---- a new function in the loaded file is defined (generation 0 of its family), then swapped
routes2 = edited('(defun route (path: str) -> str (str prefix " " path))', '(defun route (path: str) -> str (str prefix "? " path))') + '\n(defun extra () -> str "extra")\n'
write_routes(routes2)
l = c.load('src/routes.fib')
check('a new function is defined, nothing swapped', l.get('ok') is True and l.get('defined') == ['routes/extra'] and not l.get('swapped') and l.get('changed') == [] and l.get('definitions') == 5, l)
check('the new function runs', c.value('(r/extra)') == '"extra"', c.value('(r/extra)'))
write_routes(routes2.replace('(defun extra () -> str "extra")', '(defun extra () -> str "extra!")'))
l = c.load('src/routes.fib')
check('the new function is swapped by the next load', l.get('ok') is True and l.get('swapped') == ['routes/extra'] and l.get('generation') == 5 and c.value('(r/extra)') == '"extra!"', l)
l = c.load('src/routes.fib')
check('a load of an unchanged file changes nothing', l.get('ok') is True and l.get('changed') == [] and not l.get('swapped') and l.get('generation') == 5, l)

# ---- a module the program lacks; a file that cannot be read
with open(os.path.join(work, 'src', 'other.fib'), 'w') as f: f.write('(ns other)\n(defun f () -> i64 1)\n')
l = c.load('src/other.fib')
check('a module the program lacks is live/NotLoaded with the require hint', l.get('ok') is False and l.get('code') == 'live/NotLoaded' and l.get('hint') == '(require [other :as other]) at the REPL loads it'
      and 'the program has no module other' in shown_at(l, 'live/NotLoaded'), l)
l = c.load('src/nope.fib')
check('a missing file', l.get('ok') is False and l.get('code') == 'live/NoFile', l)

# ---- the main module's file: a new function is an eval of the file's changed definitions
with open(os.path.join(work, 'app.fib'), 'a') as f: f.write('\n(defun twice (n: i64) -> i64 (* 2 n))\n')
l = c.load('app.fib')
check('a load of the main file defines its new function', l.get('ok') is True and 'twice' in l.get('defined', []) and l.get('module') == 'main', l)
check('the main file\'s new function runs', c.value('(twice 21)') == '42', c.value('(twice 21)'))

# ---- the terminal client
write_routes(routes2.replace('(defun extra () -> str "extra")', '(defun extra () -> str "extra?"'))
write_routes(routes2.replace('(defun extra () -> str "extra")', '(defun extra () -> str "extra?")'))
out, r = terminal(a.sock, [':load src/routes.fib', ':status', ':reload', ':load'])
matches('terminal: :load, :status, :reload', out,
        [r'; attached to .*app\.fib \(pid %d, generation 5, catching\); .*' % a.p.pid,
         r'; src/routes\.fib: 5 definitions, 1 changed: swapped routes/extra \(generation 6\): check \d+ ms, emit \d+ ms, jit \d+ ms',
         r'; generation 6; generations kept \d+; modules kept \d+; threads: .*; values: \d+ resident; inputs \d+; tests: idle; loaded: src/routes\.fib, app\.fib',
         r'; nothing to reload: no loaded file differs from its loaded text',
         r'; :load needs a file'])
check('terminal: the swap landed', c.value('(r/extra)') == '"extra?"', c.value('(r/extra)'))

# ---- the end of the program through the socket
check('stop', c.eval('(reset! stop true)').get('ok'))
code = a.wait(60)
out = a.stdout().splitlines()
check('program ended', code == 0 and out[0] == 'app: ready' and out[-2] == 'app: stopped at route? tick' and out[-1] == '0', (code, a.stdout()[-300:], a.stderr()[-300:]))
a.clean()
shutil.rmtree(runtime, ignore_errors=True)
shutil.rmtree(work, ignore_errors=True)
print('live load: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
