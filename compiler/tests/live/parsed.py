#!/usr/bin/env python3
"""compiler/tests/live/parsed.py: the loader's memo of parsed files in a live session (expand.parsed, serve.incr `Engine.parsed` and
`Stats.parsed`; docs/design/live-incremental.md 2.6): every input loads the program again, and only the files whose text changed are
parsed again. Over a real socket against load-app.fib (as app.fib in a scratch directory) with load-routes.fib as src/routes.fib (a -I
session, 50-odd modules with the implicit library), edited on disk and loaded.
usage: parsed.py FIBC [ROOT]    Exit 0 when every check held. Each check prints `ok NAME` or `FAIL NAME: why`.
What is tested: an input after the warm-up parses at most the session's own text (`stats.parsed` <= 1 of `stats.modules` > 40); an
edit of `route` loaded parses at most two files (the loaded one and the session's text) and the call answers the edit; an edit of the
same length (one byte changed) is seen too, and so is the file put back to an earlier text; a file that does not read is refused and
the next good load answers; an expression input after the loads still parses at most one file. The load_ms of each input is printed.
The mutants of scripts/mutant-live.sh that must make this fail: parse-memo-off (every file parsed for every input), parse-memo-by-name
(an entry answered whatever the text: the loaded edit is not seen), parse-memo-by-length (texts compared by length: the one-byte edit
is not seen)."""
import json, os, shutil, socket, subprocess, sys, tempfile, time

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
here = os.path.join(root, 'compiler', 'tests', 'live')
work = tempfile.mkdtemp(prefix='live-parsed-', dir=os.environ.get('TMPDIR') or None)
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')   # a unix socket path is at most 100 bytes (serve.sock)
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')))
env.pop('FIB_LIVE', None)
failed = []
os.mkdir(os.path.join(work, 'src'))
shutil.copy(os.path.join(here, 'load-app.fib'), os.path.join(work, 'app.fib'))
routes0 = open(os.path.join(here, 'load-routes.fib')).read()
routes_file = os.path.join(work, 'src', 'routes.fib')
ROUTE = '(defun route (path: str) -> str (str prefix " " path))'

def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)

def limit():
    if sys.platform != 'linux': return
    import resource
    try: resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
    except (ValueError, OSError): pass

def write_routes(new_route):
    assert ROUTE in routes0
    with open(routes_file, 'w') as f: f.write(routes0.replace(ROUTE, new_route))

def stats(a, name):
    s = a.get('stats', {})
    print('   %s: modules %s, parsed %s, load_ms %s, total_ms %s' % (name, s.get('modules'), s.get('parsed'), s.get('load_ms'), s.get('total_ms')))
    return s

class Client:
    def __init__(self, path, timeout=600):
        self.s = socket.socket(socket.AF_UNIX); self.s.settimeout(timeout); self.s.connect(path); self.buf = b''
    def ask(self, **req):
        self.s.sendall((json.dumps(req) + '\n').encode())
        while b'\n' not in self.buf:
            d = self.s.recv(1 << 20)
            if not d: return {}
            self.buf += d
        line, self.buf = self.buf.split(b'\n', 1)
        return json.loads(line)

def start():
    out = open(os.path.join(work, 'out.txt'), 'w'); err = open(os.path.join(work, 'err.txt'), 'w')
    p = subprocess.Popen([fibc, 'run', '--live', '-I', 'src', 'app.fib'], stdout=out, stderr=err, cwd=work, env=env, preexec_fn=limit)
    for _ in range(1800):
        if p.poll() is not None: return p, None
        sock = None
        for line in open(err.name).read().splitlines():
            if line.startswith('fibc: live at '): sock = line[len('fibc: live at '):]
        if sock and os.path.exists(sock) and 'app: ready' in open(out.name).read(): return p, sock
        time.sleep(0.1)
    return p, None

write_routes(ROUTE)
p, sock = start()
check('started', sock is not None and p.poll() is None, open(os.path.join(work, 'err.txt')).read()[-800:])
if sock is None: p.kill(); sys.exit(1)
c = Client(sock)
h = c.ask(op='hello')
check('hello: the engine was warmed', h.get('warm') is True, h)

a = c.ask(op='eval', text='(r/route "x")')
s = stats(a, 'first input')
check('the first input answers', a.get('value') == '"route x"', a)
check('the first input after the warm-up parses at most the session text', s.get('modules', 0) > 40 and 0 <= s.get('parsed', -1) <= 1, s)
a = c.ask(op='eval', text='(r/greet "y")')
s = stats(a, 'second input')
check('a second input parses at most the session text', a.get('value') == '"hello y"' and 0 <= s.get('parsed', -1) <= 1, (a.get('value'), s))

write_routes('(defun route (path: str) -> str (str prefix "! " path))')
l = c.ask(op='load', file='src/routes.fib')
s = stats(l, 'load of an edit')
check('an edit loaded is swapped and parses at most two files', l.get('ok') is True and l.get('swapped') == ['routes/route'] and 1 <= s.get('parsed', -1) <= 2, l)
check('the call answers the edit', c.ask(op='eval', text='(r/route "x")').get('value') == '"route! x"')

write_routes('(defun route (path: str) -> str (str prefix "? " path))')
l = c.ask(op='load', file='src/routes.fib')
stats(l, 'load of a one-byte edit')
check('an edit of the same length is seen', l.get('ok') is True and l.get('swapped') == ['routes/route'], l)
check('the call answers the one-byte edit', c.ask(op='eval', text='(r/route "x")').get('value') == '"route? x"')

with open(routes_file, 'w') as f: f.write(routes0.replace(ROUTE, '(defun route (path: str) -> str (str prefix " " path)'))
l = c.ask(op='load', file='src/routes.fib')
check('a file that does not read is refused', l.get('ok') is False, l)
check('the refusal changed nothing', c.ask(op='eval', text='(r/route "x")').get('value') == '"route? x"')

write_routes(ROUTE)
l = c.ask(op='load', file='src/routes.fib')
stats(l, 'load of the first text again')
check('the file put back to its first text is swapped', l.get('ok') is True and l.get('swapped') == ['routes/route'], l)
check('the call answers the first text', c.ask(op='eval', text='(r/route "x")').get('value') == '"route x"')

a = c.ask(op='eval', text='(r/helper 21)')
s = stats(a, 'input after the loads')
check('an input after the loads parses at most the session text', a.get('value') == '42' and 0 <= s.get('parsed', -1) <= 1, (a.get('value'), s))

c.ask(op='eval', text='(reset! stop true)')
try: p.wait(60)
except subprocess.TimeoutExpired: p.kill()
shutil.rmtree(work, ignore_errors=True); shutil.rmtree(runtime, ignore_errors=True)
print('live parsed: %s' % ('ok' if not failed else 'FAILED (%d): %s' % (len(failed), ', '.join(failed))))
sys.exit(1 if failed else 0)
