#!/usr/bin/env python3
"""scripts/bench/dev-loop-serve.py FIBC [BEFORE_FIBC]: the latency table of the development loop (docs/design/dev-loop.md section 8.5), in seconds.
Run from the repository root, on a quiet machine, `ulimit -v 16000000`. FIBC is a stage 2 with `fibc serve`; BEFORE_FIBC (default FIBC) is used for the commands that
do not use the server. Edits are made to a scratch copy of lib/ (the tree is not touched) and passed as the first module root. Prints one line for each row:
`row | seconds (median of N, min-max)`. Takes /tmp/fibsuite.lock without waiting (a held lock is reported and the run goes on).
Rows: cold `run hello`; `emit hello` (the front end alone; there is no `fibc check` in the tree); `test` of one scenario; `cases` of one case; after an edit of one function
of a large module: `run` and `test` of a spec that uses it with no server, `check` of the module with no server (the client's fallback), and with a warm server: `check` of
the module (lib/fib/json/codec.fib, lib/fib/tls/chain.fib), `check` of the spec, `run` of hello, `test` of one scenario, `eval` of a new definition and of an expression."""
import json, os, shutil, socket, statistics, subprocess, sys, time, resource, fcntl

fibc = os.path.abspath(sys.argv[1]); before = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else fibc
root = os.getcwd()
scratch = os.path.expanduser('~/.cache/fibber-scratch/dv/bench-lib')
sock = os.path.expanduser('~/.cache/fibber-scratch/dv/bench2.sock')
env = dict(os.environ, FIB_LIB=os.path.join(root, 'lib'))
limit = lambda: resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
try: lock = open('/tmp/fibsuite.lock', 'w'); fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
except Exception: print('note: /tmp/fibsuite.lock is held; measuring anyway (expect noise)', flush=True)
if os.path.exists(scratch): shutil.rmtree(scratch)
shutil.copytree(os.path.join(root, 'lib'), scratch)

def sh(cmd, **kw):
    t0 = time.time(); r = subprocess.run(cmd, capture_output=True, text=True, env=env, preexec_fn=limit, cwd=root, **kw); return time.time() - t0, r
def row(name, ts, note=''):
    print('| %-62s | %6.3f | %6.3f-%6.3f | %s' % (name, statistics.median(ts), min(ts), max(ts), note), flush=True)
def times(cmd, n=5):
    return [sh(cmd)[0] for _ in range(n)]
import re
MARKS = {'codec': (r'": missing, expected(Q*)', '": missing, expected%s'), 'chain': (r'"an RSA key of(Q*) "', '"an RSA key of%s "'),
         'access': (r'\(\(JBool _\) "boolean(Q*)"', '((JBool _) "boolean%s"')}
def bump(path, kind):
    """One more character in a string literal inside one function (codec.fib `mismatch`, chain.fib `check-keys`, access.fib `json-type`): an edit of one function."""
    rx, fmt = MARKS[kind]; t = open(path).read(); m = re.search(rx, t); assert m, (path, rx)
    open(path, 'w').write(t[:m.start()] + fmt % ('Q' * (len(m.group(1)) + 1)) + t[m.end():])

print('| %-62s | %6s | %-13s |' % ('row (seconds)', 'median', 'min-max'))
row('cold `run hello`', times([before, 'run', 'scripts/bench/hello.fib']))
row('`emit hello` (front end alone)', times([before, 'emit', 'scripts/bench/hello.fib']))
row('`test specs/json-api-spec.fib --only lookups` (one scenario)', times([before, 'test', 'specs/json-api-spec.fib', '--only', 'lookups'], 3))
row('`cases cases/ownership --only 01-` (one case)', times([before, 'cases', 'cases/ownership', '--only', '01-']))

codec = os.path.join(scratch, 'fib/json/codec.fib'); chain = os.path.join(scratch, 'fib/tls/chain.fib'); access = os.path.join(scratch, 'fib/json/access.fib')
spec = os.path.join(root, 'specs/json-api-spec.fib')
def after_edits(cmd, n=4, kind='codec', path=codec):
    t = []
    for _ in range(n): bump(path, kind); t.append(sh(cmd)[0])
    return t
row('after one-function edit of fib.json.codec: `check` with no server (client fallback)', after_edits([fibc, '--server', 'check', codec, '-I', scratch, '--socket', '/nonexistent', '-q']), 'cold in-process engine, whole check')
row('after an edit of fib.json.access: `run specs/json-api-spec.fib` (no server)', after_edits([before, 'run', '-I', scratch, spec], 3, 'access', access), 'the loop today')
row('after the same edit: `test` one scenario of it (no server)', after_edits([before, 'test', '-I', scratch, spec, '--only', 'lookups'], 3, 'access', access))

# the server
if os.path.exists(sock): os.unlink(sock)
srv = subprocess.Popen([fibc, 'serve', '--socket', sock, '-I', scratch], env=env, cwd=root, preexec_fn=limit, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(100):
    if os.path.exists(sock): break
    time.sleep(0.05)
def ask(req, timeout=300):
    t0 = time.time(); s = socket.socket(socket.AF_UNIX); s.settimeout(timeout); s.connect(sock)
    s.sendall((json.dumps(req) + '\n').encode()); buf = b''
    while not buf.endswith(b'\n'):
        d = s.recv(1 << 22)
        if not d: break
        buf += d
    s.close(); return time.time() - t0, json.loads(buf)
try:
    for name, path, kind in (('fib.json.codec', codec, 'codec'), ('fib.tls.chain', chain, 'chain')):
        t0, r = ask({'op': 'check', 'file': path, 'roots': [scratch]})
        print('| %-62s | %6.3f | %6s | cold server: %s' % ('`check` %s, first time (server cold)' % name, t0, '', {k: r['stats'][k] for k in ('modules', 'expanded', 'lower_ms', 'check_ms')}), flush=True)
        ts = []
        for k in range(7):
            bump(path, kind); t0, r = ask({'op': 'check', 'file': path, 'roots': [scratch]}); ts.append(t0); last = r['stats']
        row('`check` %s after a one-function edit (warm server)' % name, ts[1:], 'expanded %d of %d, lower %d ms, check %d ms, ok=%s' % (last['expanded'], last['modules'], last['lower_ms'], last['check_ms'], r['ok']))
        t0, r = ask({'op': 'check', 'file': path, 'roots': [scratch]}); row('`check` %s, text already seen (verdict kept)' % name, [t0])
    t0, r = ask({'op': 'check', 'file': spec, 'roots': [scratch]}); print('| %-62s | %6.3f | %6s | %s' % ('`check` the spec, first time', t0, '', {k: r['stats'][k] for k in ('modules', 'expanded', 'check_ms')}), flush=True)
    ts = []
    for k in range(4):
        bump(access, 'access'); t0, r = ask({'op': 'check', 'file': spec, 'roots': [scratch]}); ts.append(t0)
    row('`check` json-api-spec after an edit of fib.json.access (warm server)', ts[1:], 'expanded %d of %d, check %d ms' % (r['stats']['expanded'], r['stats']['modules'], r['stats']['check_ms']))
    ts = []
    for k in range(3):
        bump(access, 'access'); t0, r = ask({'op': 'test', 'file': spec, 'roots': [scratch], 'only': 'lookups'}); ts.append(t0)
    row('`test` one scenario of json-api-spec after an edit of access (warm server)', ts, 'exit %s; front end %d ms of it' % (r.get('exit'), r['stats']['total_ms']))
    ts = [ask({'op': 'run', 'file': 'scripts/bench/hello.fib'})[0] for _ in range(5)]
    row('`run hello` through the server', ts)
    ask({'op': 'eval', 'session': 'b', 'text': '(defun sq (x: i64) -> i64 (* x x))'})
    ts = [ask({'op': 'eval', 'session': 'b', 'text': '(sq %d)' % k})[0] for k in range(5)]
    row('`eval` of an expression in a session with one definition', ts)
    ts = [ask({'op': 'eval', 'session': 'b', 'text': '(defun sq (x: i64) -> i64 (+ x %d))' % k})[0] for k in range(5)]
    row('`eval` of a redefinition (the program of the session is checked)', ts)
    import resource as rs
    pid = ask({'op': 'stats'})[1]['pid']
    rss = int([l for l in open('/proc/%d/status' % pid).read().split('\n') if l.startswith('VmRSS')][0].split()[1]) / 1024
    print('server worker resident set: %.0f MB (entries %d)' % (rss, ask({'op': 'stats'})[1]['entries']))
finally:
    try: ask({'op': 'shutdown'})
    except Exception: pass
    srv.wait(timeout=20)
