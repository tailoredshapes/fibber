#!/usr/bin/env python3
"""compiler/tests/live/statics.py: closure constants and vtables named by content in live mode (package LE2 of
docs/design/live-incremental.md 2.2 item 3; emit.statics `statics-closure`/`statics-vtable`, serve.livesess `export-resident`,
serve.livesplit `code-static?`), over a real socket against statics-app.fib, run under FIB_LIVE_DEBUG=cut (an input's answer lists the
names its cuts defined) and read through `status` with `statics: true` (the closures and vtables of the running program by name).
usage: statics.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested, in one app:
  - the base names its closures `clo.CODE` and its vtables `vt.NAME`: one closure of `inc1` (held by the def `ops`), one vtable per
    instance of `Shape`, no numbered one;
  - `inc1` swapped: the def's closure runs the new body; `apply-op` swapped to a body that calls through the def and makes a closure of
    `cube` (a value the base never made): the cut defines no closure of `inc1`, `dbl` or `neg` (the base's are declared), the session
    still has one closure of `inc1` and now one of `cube`; `inc1` and `cube` swapped again: the swapped body's calls through the def and
    through its own closure reach the new generations;
  - a method swap (`impl Shape Sq`, its new body making a `(dyn Shape Rect)`): the cut defines no vtable (the base's is declared: its
    slots name the methods' stubs), the session has one vtable of each instance;
  - a new instance (`Tri`) in an input: one new vtable; a later input that uses it defines none and the count stays.
The mutants of scripts/mutant-live.sh that must make this fail: closure-renumbered (closures keep the program's count in live mode: the
swapped body's closure of `cube` takes the number the base gave `pick`'s closure of `sq1`, and the cut declares the base's: the body
calls `sq1`), vtable-copied (a cut copies the base's vtable instead of declaring it: the method swap's cut defines one)."""
import json, os, re, shutil, socket, subprocess, sys, tempfile, time

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
app = os.path.join(here, 'statics-app.fib')
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')   # a unix socket path is at most 100 bytes (serve.sock)
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')), FIB_LIVE_DEBUG='cut')
for k in ('FIB_LIVE', 'FIB_LIVE_EMIT', 'FIB_LIVE_QUEUE'): env.pop(k, None)
failed = []

def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)

def limit():
    if sys.platform != 'linux': return
    import resource
    try: resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
    except (ValueError, OSError): pass

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
    def statics(self): return (self.ask(op='status', statics=True) or {}).get('statics', [])

def named(statics, prefix, word):
    """The statics of `prefix` whose name has `word` as a dotted part."""
    return [s for s in statics if s.startswith(prefix) and word in s[len(prefix):].split('.')]

out = tempfile.NamedTemporaryFile(prefix='live-out-', delete=False)
err = tempfile.NamedTemporaryFile(prefix='live-err-', delete=False)
p = subprocess.Popen([fibc, 'run', '--live', app], stdout=out, stderr=err, cwd=root, env=env, preexec_fn=limit)
sock = None
for _ in range(1800):
    if p.poll() is not None: break
    for line in open(err.name).read().splitlines():
        if line.startswith('fibc: live at '): sock = line[len('fibc: live at '):]
    if sock and os.path.exists(sock) and 'app: ready' in open(out.name).read(): break
    time.sleep(0.1)
try:
    if not (sock and p.poll() is None):
        check('app started', False, open(err.name).read()[-2000:])
        raise SystemExit
    check('app started', 'app: ready 2 2 -1 8 5 19' in open(out.name).read(), open(out.name).read())
    c = Client(sock)

    st = c.statics()
    check('base: closures named by code', len(named(st, 'clo.', 'inc1')) == 1 and not [s for s in st if re.fullmatch(r'(clo|vt)\.\d+', s)], st)
    check('base: one vtable per instance', len(named(st, 'vt.', 'Sq')) == 1 and len(named(st, 'vt.', 'Rect')) == 1, st)
    check('base: no closure of cube', not named(st, 'clo.', 'cube'), st)
    check('apply-op through the def', c.value('(apply-op 0 5)') == '6')

    s = c.eval('(defun inc1 (n: i64) -> i64 (+ n 100))')
    check('swap inc1', s.get('swapped') == ['inc1'], s)
    check("the def's closure runs the new inc1", c.value('(apply-op 0 5)') == '105')

    s = c.eval('(defun apply-op (i: i64 n: i64) -> i64 (+ ((nth ops i) n) (let ((f: (fn (i64) i64) cube)) (f 2))))')
    cut = s.get('cut', [])
    check('swap apply-op', s.get('swapped') == ['apply-op'], s)
    check("apply-op's cut declares the base's closures", cut and not [n for n in cut if n.startswith('clo.') and set(n.split('.')) & {'inc1', 'dbl', 'neg'}], cut)
    st = c.statics()
    check('one closure of inc1 and one of cube', len(named(st, 'clo.', 'inc1')) == 1 and len(named(st, 'clo.', 'cube')) == 1, st)
    v = (c.value('(apply-op 0 5)'), c.value('(apply-op 2 5)'))
    check('the swapped body through the def and its own closure', v == ('113', '3'), v)

    check('swap inc1 again', c.eval('(defun inc1 (n: i64) -> i64 (+ n 1000))').get('swapped') == ['inc1'])
    v = (c.value('(apply-op 0 5)'), c.value('(apply-op 1 5)'))
    check("the swapped body's call through the def reaches the new inc1", v == ('1013', '18'), v)
    s = c.eval('(defun cube (n: i64) -> i64 (* n 10))')
    check('swap cube', s.get('swapped') == ['cube'], s)
    v = c.value('(apply-op 0 5)')
    check("the swapped body's own closure reaches the new cube", v == '1025', v)

    check('areas before', c.value('(areas)') == '19')
    vts = [x for x in c.statics() if x.startswith('vt.')]
    s = c.eval('(impl Shape Sq (area (self) (+ (* 10 (. self side)) (shape-area (dyn Shape (Rect 0 0))))))')
    cut = s.get('cut', [])
    check('swap a method', s.get('ok') and any(x.startswith('m.') for x in s.get('swapped', [])), s)
    check("the method swap's cut defines no vtable", cut and not [n for n in cut if n.startswith('vt.')], cut)
    check('the swapped method through the vtable', c.value('(areas)') == '40')
    st = c.statics()
    check('one vtable of Sq after the method swap', len(named(st, 'vt.', 'Sq')) == 1 and len([x for x in st if x.startswith('vt.')]) == len(vts), st)

    vt0 = len([x for x in c.statics() if x.startswith('vt.')])
    check('a new type', c.eval('(defrecord Tri (b: i64 h: i64))').get('ok'))
    s = c.eval('(impl Shape Tri (area (self) (* (. self b) (. self h))))')
    check('a new instance', s.get('ok'), s)
    s = c.eval('(shape-area (dyn Shape (Tri 4 3)))')
    check('the new instance through dyn', s.get('value') == '12', s)
    check('its input defines the vtable', named(c.statics(), 'vt.', 'Tri') and [n for n in s.get('cut', []) if n.startswith('vt.')] == named(c.statics(), 'vt.', 'Tri'), s.get('cut'))
    st = c.statics()
    check('one new vtable', len(named(st, 'vt.', 'Tri')) == 1 and len([x for x in st if x.startswith('vt.')]) == vt0 + 1, st)
    s = c.eval('(shape-area (dyn Shape (Tri 2 3)))')
    check('a later input declares it', s.get('value') == '6' and not [n for n in s.get('cut', []) if n.startswith('vt.')], s)
    check('still one vtable of Tri', len(named(c.statics(), 'vt.', 'Tri')) == 1)

    c.eval('(reset! stop true)')
    try: code = p.wait(60)
    except subprocess.TimeoutExpired: p.kill(); code = None
    check('the app ends with status 0', code == 0, (code, open(err.name).read()[-1000:]))
finally:
    if p.poll() is None: p.kill()
    for f in (out.name, err.name):
        try: os.unlink(f)
        except OSError: pass
    shutil.rmtree(runtime, ignore_errors=True)
print('statics: %d failed' % len(failed))
sys.exit(1 if failed else 0)
