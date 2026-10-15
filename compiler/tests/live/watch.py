#!/usr/bin/env python3
"""compiler/tests/live/watch.py: the checks of `watch` of a live session (docs/design/live-meshql.md 3.8, package LT5; serve.livewatch,
serve.live `watch-step`, serve.liveclient `:watch`) over a real socket against watch-app.fib (as app.fib in a scratch directory, the
session's working directory) with watch-handler.fib as src/handler.fib (a -I session) and a feature file under features/, edited on disk
while the connection watches (each write is a new file renamed into place, as an editor saves).
usage: watch.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested: `watch on` with `dirs` answers how many files it watches; a change before any test reruns nothing (no event in 2 s); after
a `test` (the row fails: the handler answers "v1"), an edit of the handler's file brings a `load` event (`swapped` handler/handle) and then
a `test` event whose row passes (within 10 s; the design's 1.5 s is the figure on an idle machine); a change of the feature file brings a
`test` event and no `load`; `status` is answered while watching and says so; an edit the checker refuses brings the refusal as a `load`
event and no `test` event in 4 s; the good edit after it is loaded and rerun; a file written in place slowly (8 bytes per 10 ms) is loaded once, whole, never
half-written (the debounce); `watch off`: an edit brings no event in 3 s and is not loaded;
the terminal client's `:watch on DIR` and `:watch off` lines; the app ends through the socket.
The mutants of scripts/mutant-live.sh that must make this fail: watch-no-load, watch-reruns-on-refusal, watch-no-debounce."""
import json, os, shutil, socket, subprocess, sys, tempfile, time

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
work = tempfile.mkdtemp(prefix='live-watch-', dir=os.environ.get('TMPDIR') or None)
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')   # a unix socket path is at most 100 bytes (serve.sock)
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')))
env.pop('FIB_LIVE', None)
failed = []
os.mkdir(os.path.join(work, 'src'))
os.mkdir(os.path.join(work, 'features'))
shutil.copy(os.path.join(here, 'watch-app.fib'), os.path.join(work, 'app.fib'))
handler0 = open(os.path.join(here, 'watch-handler.fib')).read()
handler_file = os.path.join(work, 'src', 'handler.fib')
feature_file = os.path.join(work, 'features', 'watch.feature')

def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)

def limit():
    if sys.platform != 'linux': return
    import resource
    try: resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
    except (ValueError, OSError): pass

def save(path, text):
    """as an editor saves: a new file beside it (a dot name: not watched) renamed into place"""
    tmp = os.path.join(os.path.dirname(path), '.save-' + os.path.basename(path))
    with open(tmp, 'w') as f: f.write(text)
    os.replace(tmp, path)

def handler(body):
    return handler0.replace('(defun handle (n: i64) -> str (str "v" n))', body)

save(handler_file, handler0)
save(feature_file, 'Feature: watch\n')

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
    """answers are returned by `ask`; events (`{"event": ..}`) are queued in `events` as they come"""
    def __init__(self, path):
        self.s = socket.socket(socket.AF_UNIX); self.s.connect(path); self.buf = b''; self.events = []
    def line(self, timeout):
        self.s.settimeout(max(timeout, 0.01))
        while b'\n' not in self.buf:
            try: d = self.s.recv(1 << 20)
            except socket.timeout: return None
            if not d: return None
            self.buf += d
        line, self.buf = self.buf.split(b'\n', 1)
        return json.loads(line)
    def ask(self, **req):
        self.s.sendall((json.dumps(req) + '\n').encode())
        end = time.time() + 600
        while time.time() < end:
            v = self.line(end - time.time())
            if v is None: return {}
            if 'event' in v: self.events.append(v)
            else: return v
        return {}
    def wait_events(self, secs, until=None):
        """the events that come within `secs` (or until one satisfies `until`), the queued ones first"""
        got, self.events = self.events, []
        end = time.time() + secs
        while not (until and any(until(e) for e in got)) and time.time() < end:
            v = self.line(end - time.time())
            if v is not None: got.append(v)
        return got
    def close(self): self.s.close()

def is_test(e): return e.get('event') == 'test'

def row(a, text):
    for r in (a.get('report') or {}).get('results', []):
        if r['text'] == text: return r
    return {}

a = App('app.fib', ['-I', 'src'])
check('started', a.sock is not None and a.alive() and 'app: ready' in a.stdout(), 'stderr: %s stdout: %s' % (a.stderr()[-800:], a.stdout()))
if a.sock is None or not a.alive(): a.p.kill(); sys.exit(1)
c = Client(a.sock)

w = c.ask(op='watch', on=True, dirs=['features'])
check('watch on: the files watched', w.get('ok') is True and w.get('watching') is True and w.get('files') == 3, w)
save(feature_file, 'Feature: watch\n# before any test\n')
ev = c.wait_events(2)
check('a change before any test reruns nothing', ev == [], ev)

t = c.ask(op='test')
check('the first run: the row fails ("v1")', t.get('ok') is True and row(t, 'the handler').get('status') == 'fail', t)

# ---- the handler's file edited: loaded, then the last test rerun
t0 = time.time()
save(handler_file, handler('(defun handle (n: i64) -> str (str "v" (+ n 1)))'))
ev = c.wait_events(60, is_test)
secs = time.time() - t0
loads = [e for e in ev if e.get('event') == 'load']
tests = [e for e in ev if is_test(e)]
check('handler edit: a load event swapping handler/handle', len(loads) == 1 and loads[0].get('ok') is True and loads[0].get('swapped') == ['handler/handle'], ev)
check('handler edit: the rerun\'s row reflects the edit', len(tests) == 1 and row(tests[0], 'the handler').get('status') == 'pass' and tests[0].get('id') == 'watch', ev)
check('handler edit: the rerun within 10 s (%.2f s)' % secs, secs <= 10, secs)

# ---- the feature file changed: a rerun with no load
save(feature_file, 'Feature: watch\n# edited\n')
ev = c.wait_events(60, is_test)
check('feature edit: a test event and no load', [e.get('event') for e in ev] == ['test'] and 'swapped' not in ev[0] and row(ev[0], 'the handler').get('status') == 'pass', ev)

s = c.ask(op='status')
check('status while watching', s.get('ok') is True and (s.get('watch') or {}).get('on') is True and (s.get('watch') or {}).get('dirs') == ['features'], s)

# ---- a refused edit: the refusal, and no rerun
save(handler_file, handler('(defun handle (n: i64) -> str 42)'))
ev = c.wait_events(60, lambda e: e.get('event') == 'load')
ev += c.wait_events(4)
check('refused edit: the refusal as a load event', len(ev) >= 1 and ev[0].get('event') == 'load' and ev[0].get('ok') is False and ev[0].get('code'), ev)
check('refused edit: no rerun', not any(is_test(e) for e in ev), ev)
check('refused edit: the program keeps the old handler', c.ask(op='eval', text='(h/handle 1)').get('value') == '"v2"')

save(handler_file, handler('(defun handle (n: i64) -> str (str "v" (+ 1 n)))'))
ev = c.wait_events(60, is_test)
check('the good edit after it: loaded and rerun', [e.get('event') for e in ev] == ['load', 'test'] and ev[0].get('ok') is True and row(ev[1], 'the handler').get('status') == 'pass', ev)

# ---- the debounce: the handler's file written in place slowly (every prefix of the new text is refused: an unclosed form); no scan may
# load it half-written: one load, accepted, then the rerun
slow = handler('(defun handle (n: i64) -> str\n  ;; ' + 'padding ' * 150 + '\n  (str "v" (+ n 1)))')
with open(handler_file, 'w') as f:
    for i in range(0, len(slow), 8):
        f.write(slow[i:i + 8]); f.flush(); time.sleep(0.01)
ev = c.wait_events(60, is_test)
check('a slow write: loaded once whole, never half-written, then rerun', [e.get('event') for e in ev] == ['load', 'test'] and ev[0].get('ok') is True
      and ev[0].get('swapped') == ['handler/handle'] and row(ev[1], 'the handler').get('status') == 'pass', ev)

# ---- watch off
w = c.ask(op='watch', on=False)
check('watch off', w.get('ok') is True and w.get('watching') is False, w)
save(handler_file, handler('(defun handle (n: i64) -> str (str "w" n))'))
ev = c.wait_events(3)
check('watch off: a change brings nothing in 3 s', ev == [], ev)
check('watch off: the change is not loaded', c.ask(op='eval', text='(h/handle 1)').get('value') == '"v2"')

# ---- the terminal client (the raw client is closed meanwhile: the program serves one connection at a time)
c.close()
r = subprocess.run([fibc, 'repl', '--connect', a.sock], input=':watch on features\n:watch off\n:watch sideways\n:quit\n', capture_output=True, text=True, cwd=work, env=env, timeout=600, preexec_fn=limit)
out = r.stdout.splitlines()
check('terminal: :watch on, :watch off, a bad word', out[1:] == ['; watching 3 files', '; watch off', '; :watch on [DIR..] or :watch off'], (out, r.stderr[-400:]))
c = Client(a.sock)

check('stop', c.ask(op='eval', text='(reset! stop true)').get('ok'))
code = a.wait(60)
check('program ended', code == 0 and a.stdout().splitlines()[-2:] == ['app: stopped', '0'], (code, a.stdout()[-300:], a.stderr()[-300:]))
a.clean()
shutil.rmtree(runtime, ignore_errors=True)
shutil.rmtree(work, ignore_errors=True)
print('live watch: %s' % ('ok' if not failed else 'FAILED ' + ' '.join(failed)))
sys.exit(1 if failed else 0)
