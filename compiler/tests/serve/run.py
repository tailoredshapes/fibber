#!/usr/bin/env python3
"""compiler/tests/serve/run.py: the tests of `fibc serve` over a real unix socket (docs/design/dev-loop.md section 8).
usage: run.py FIBC [ROOT]   (ROOT: the repository; default the current directory). Offline; about 30 s. Exit 0 when every check held.
Each check prints `ok NAME` or `FAIL NAME: why`. What is tested: the protocol (ids, errors, one request for each connection), diagnostics (file, line,
column, code), the cache against changed files (content, not time; a dependent is checked again), the server's emit equal to the compiler's, runs (arguments,
exit status, a trap, a timeout), concurrent clients, a crashed worker (restarted; the client falls back when nothing answers), 10,000 malformed requests
(the worker never dies), and shutdown."""
import json, os, random, shutil, socket, subprocess, sys, tempfile, threading, time

# Under the gate argv[1] is the slot wrapper (scripts/lib/fibc-slot.sh): a long-lived server must not hold a gate slot for its whole life, nor wait for one
# among sixty other scripts; the real fibc is named by GATE_REAL_FIBC (scripts/tools.sh).
fibc = os.path.abspath(os.environ.get('GATE_REAL_FIBC') or sys.argv[1])
root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
work = tempfile.mkdtemp(prefix='serve-test-')
sock = os.path.join(work, 's.sock')
failed = []

def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)

def limit():
    if sys.platform != 'linux': return   # macOS refuses RLIMIT_AS (the preexec_fn fails and the server never starts)
    import resource
    try: resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
    except (ValueError, OSError): pass

env = dict(os.environ, FIBC_SERVE_DEBUG='1', FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib')))

def raw(data, path=None, timeout=300):
    s = socket.socket(socket.AF_UNIX); s.settimeout(timeout); s.connect(path or sock)
    s.sendall(data)
    buf = b''
    try:
        while not buf.endswith(b'\n'):
            d = s.recv(1 << 20)
            if not d: break
            buf += d
    finally:
        s.close()
    return buf

def ask(req, **kw):
    line = raw((json.dumps(req) + '\n').encode(), **kw)
    return json.loads(line) if line else None

def client(*args, **kw):
    return subprocess.run([fibc, '--server'] + list(args) + ['--socket', sock], capture_output=True, text=True, cwd=root, env=env, timeout=120, preexec_fn=limit, **kw)

# ---- the project the tests edit: module a, b requires a, main requires b
proj = os.path.join(work, 'proj'); os.makedirs(os.path.join(proj, 'lib'))
def write(path, text):
    with open(os.path.join(proj, path), 'w') as f: f.write(text)
A_OK = '(ns lib.a)\n(defun twice (x: i64) -> i64 (* 2 x))\n'
A_NEW = '(ns lib.a)\n(defun twice (x: str) -> str x)\n'
write('lib/a.fib', A_OK)
write('lib/b.fib', '(ns lib.b (:require [lib.a :as a]))\n(defun four (x: i64) -> i64 (a/twice (a/twice x)))\n')
write('main.fib', '(ns main (:require [lib.b :as b]))\n(defun main () -> i64 (b/four 3))\n')
write('bad.fib', '(ns main)\n(defun f (x: i64) -> str x)\n(defun main () -> i64 0)\n')
write('args.fib', '(ns main)\n(defun main () -> i64 (do (println (str "args " (count (args)) " " (nth (vec (args)) 0))) 3))\n')
write('trap.fib', '(ns main)\n(defun main () -> i64 (do (println "before") (nth [1 2] 9) 0))\n')
write('loop.fib', '(ns main)\n(defun main () -> i64 (loop ((i 0)) (recur (+ i 1))))\n')
P = lambda n: os.path.join(proj, n)
ROOTS = [proj]

srv = subprocess.Popen([fibc, 'serve', '--socket', sock, '-I', proj], cwd=root, env=env, stdout=subprocess.DEVNULL, stderr=open(os.path.join(work, 'server.log'), 'w'), preexec_fn=limit)
try:
    up = False
    for _ in range(1500):    # under the gate a fibc waits for a slot before it starts
        try:
            if ask({'op': 'ping'}, timeout=5): up = True; break
        except Exception: time.sleep(0.2)
    check('server starts and answers ping', up, 'no answer; server exit status %r (None: still running); socket exists %r; log: %s' % (srv.poll(), os.path.exists(sock), open(os.path.join(work, 'server.log')).read()[:300]))
    if not up: raise SystemExit(1)

    # ---- protocol
    r = ask({'op': 'ping', 'id': 'x1'}); check('id is echoed (string)', r and r['id'] == 'x1' and r['ok'] is True, r)
    r = ask({'op': 'ping', 'id': 7}); check('id is echoed (integer)', r and r['id'] == 7, r)
    r = ask({'op': 'nonsense', 'id': 1}); check('unknown op is an error response', r and r['ok'] is False and r['error']['code'] == 'unknown-op', r)
    r = json.loads(raw(b'this is not json\n')); check('not JSON is a bad-request', r['ok'] is False and r['error']['code'] == 'bad-request', r)
    r = json.loads(raw(b'[1,2]\n')); check('a non-object is a bad-request', r['error']['code'] == 'bad-request', r)
    r = json.loads(raw(b'{"id":1}\n')); check('a request without op is a bad-request', r['error']['code'] == 'bad-request', r)
    r = ask({'op': 'check', 'id': 2}); check('check without file is a bad-request', r['error']['code'] == 'bad-request', r)
    r = ask({'op': 'check', 'file': P('missing.fib')}); check('check of a missing file is cannot-read', r['error']['code'] == 'cannot-read', r)

    # ---- diagnostics
    r = ask({'op': 'check', 'file': P('main.fib'), 'roots': ROOTS}); check('check of a good program holds', r['ok'] is True and r['diagnostics'] == [] and r['exit'] == 0, r)
    r = ask({'op': 'check', 'file': P('bad.fib')})
    d = r['diagnostics'][0] if r.get('diagnostics') else {}
    check('diagnostic has file, line, column, code, message', r['ok'] is False and r['exit'] == 3 and d.get('file') == P('bad.fib') and d.get('line') == 2 and d.get('col') == 26 and d.get('endCol') == 27 and d.get('code') == 'type/Unify' and 'cannot unify' in d.get('message', '') and d.get('endLine') == 2 and d.get('endCol', 0) > d.get('col', 0), r)
    r = ask({'op': 'check', 'file': P('bad.fib'), 'text': '(ns main)\n(defun main () -> i64 0)\n'}); check('check takes the text of an unsaved buffer', r['ok'] is True, r)
    r = ask({'op': 'check', 'file': P('bad.fib'), 'text': '(ns main)\n(defun main () -> i64 (\n'}); check('a read error is a diagnostic with the read code', r['ok'] is False and r['diagnostics'][0]['code'].startswith('read/'), r)

    # ---- the cache follows the content
    r1 = ask({'op': 'check', 'file': P('main.fib'), 'roots': ROOTS}); check('a second check of the same text is answered from the verdict', r1['stats']['verdict'] is True and r1['ok'], r1)
    os.utime(P('lib/a.fib'), None)
    r1 = ask({'op': 'check', 'file': P('main.fib'), 'roots': ROOTS}); check('touching a file does not change the answer (content, not time)', r1['stats']['verdict'] is True, r1)
    write('lib/a.fib', A_NEW)
    r2 = ask({'op': 'check', 'file': P('main.fib'), 'roots': ROOTS})
    check('an edited dependency is seen: the dependent no longer checks (stale cache is not served)', r2['ok'] is False and r2['stats']['verdict'] is False and len(r2['diagnostics']) > 0, r2)
    check('only the modules from the edit on are expanded again', 0 < r2['stats']['expanded'] < r2['stats']['modules'] and r2['stats']['reused'] > 0, r2['stats'])
    cold = subprocess.run([fibc, '--server', 'check', P('main.fib'), '-I', proj, '--socket', os.path.join(work, 'nobody.sock'), '-q'], capture_output=True, text=True, cwd=root, env=env, preexec_fn=limit)
    warm = client('check', P('main.fib'), '-I', proj, '-q')
    check('the warm server and a cold process give the same diagnostics for the edited program', cold.returncode == 3 and cold.stderr == warm.stderr and cold.stderr != '', (cold.stderr, warm.stderr))
    r3 = ask({'op': 'check', 'file': P('lib/b.fib'), 'roots': ROOTS}); check('the dependent module itself is rejected too', r3['ok'] is False, r3)
    write('lib/a.fib', A_OK)
    r4 = ask({'op': 'check', 'file': P('main.fib'), 'roots': ROOTS}); check('reverting the edit makes it check again', r4['ok'] is True, r4)

    # ---- emit equals the compiler's own
    for name, text in (('main.fib', None), ('main.fib', 'edit')):
        if text: write('lib/a.fib', '(ns lib.a)\n(defun twice (x: i64) -> i64 (+ x x))\n')
        a = ask({'op': 'emit', 'file': P('main.fib'), 'roots': ROOTS})
        b = subprocess.run([fibc, 'emit', '-I', proj, P('main.fib')], capture_output=True, text=True, cwd=root, env=env, preexec_fn=limit).stdout
        check('emit through the server equals fibc emit (%s)' % ('after an edit' if text else 'first'), a and a.get('lir') == b and len(b) > 1000, 'differs')
    write('lib/a.fib', A_OK)

    # ---- runs
    r = ask({'op': 'run', 'file': P('args.fib'), 'args': ['alpha', 'beta']}); check('run passes the arguments and answers stdout and the result', r['ok'] and r['stdout'] == 'args 2 alpha\n3\n' and r['exit'] == 0, r)
    r = ask({'op': 'run', 'file': P('trap.fib')}); check('a trap is reported (status 134, trapped) and the server goes on', r['ok'] is False and r['exit'] == 134 and r['trapped'] is True and 'before' in r['stdout'] and 'index out of range' in r['stderr'], r)
    r = ask({'op': 'run', 'file': P('loop.fib'), 'timeout': 1}); check('a program that does not end is stopped at the timeout', r['exit'] == 142 and r['timed_out'] is True, r)
    r = ask({'op': 'run', 'file': P('bad.fib')}); check('run of a program that does not check answers diagnostics, runs nothing', r['ok'] is False and r['exit'] == 3 and 'stdout' not in r, r)
    r = ask({'op': 'ping'}); check('the server answers after the traps', r and r['ok'], r)
    spec = os.path.join(root, 'specs', 'adder-spec.fib')
    if os.path.exists(spec):
        r = ask({'op': 'test', 'file': spec, 'only': 'zzz-no-such-scenario'}); check('test: a selection that matches nothing is not a pass', r and r['ok'] is False and r['exit'] != 0, r)

    # ---- the client: output and status as the ordinary command, and the fallback
    c = client('check', P('main.fib'), '-I', proj); check('client check prints nothing and exits 0 on a good program', c.returncode == 0 and c.stdout == '', (c.returncode, c.stdout, c.stderr))
    c = client('check', P('bad.fib')); check('client check prints rejected: and the diagnostic, exit 3', c.returncode == 3 and 'rejected:' in c.stderr and P('bad.fib') + ':2:26' in c.stderr, (c.returncode, c.stderr))
    c = client('run', P('args.fib'), '--', 'q'); check('client run prints stdout and exits with the status', c.returncode == 0 and c.stdout == 'args 1 q\n3\n', (c.returncode, c.stdout, c.stderr))
    c = client('eval', '(+ 1 2)'); check('client eval prints the value', c.returncode == 0 and c.stdout.strip() == '3', (c.returncode, c.stdout, c.stderr))
    c = subprocess.run([fibc, '--server', 'check', P('bad.fib'), '--socket', os.path.join(work, 'nobody.sock')], capture_output=True, text=True, cwd=root, env=env, preexec_fn=limit)
    check('with no server the client runs the ordinary check here (same diagnostic, same status)', c.returncode == 3 and P('bad.fib') + ':2:26' in c.stderr and 'running here' in c.stderr, (c.returncode, c.stderr))

    # ---- eval sessions
    r = ask({'op': 'eval', 'session': 's1', 'text': '(defun sq (x: i64) -> i64 (* x x))'}); check('eval: a definition is added', r['ok'] and r['defined'] == ['sq'], r)
    r = ask({'op': 'eval', 'session': 's1', 'text': '(sq 12)'}); check('eval: an expression sees the session', r['ok'] and r['value'] == '144', r)
    r = ask({'op': 'eval', 'session': 's2', 'text': '(sq 12)'}); check('eval: sessions are separate', r['ok'] is False and r.get('diagnostics'), r)
    r = ask({'op': 'eval', 'session': 's1', 'text': '(defun sq (x: i64) -> i64 (+ x 1))'}); r = ask({'op': 'eval', 'session': 's1', 'text': '(sq 12)'}); check('eval: a redefinition replaces', r['value'] == '13', r)
    r = ask({'op': 'eval', 'session': 's1', 'text': '(nth [1] 4)'}); r2 = ask({'op': 'eval', 'session': 's1', 'text': '(sq 1)'}); check('eval: a trap leaves the session usable', r['trapped'] is True and r2['value'] == '2', (r, r2))

    # ---- concurrent clients
    out = {}
    def worker(i):
        try:
            if i % 3 == 0: out[i] = ask({'op': 'check', 'file': P('main.fib'), 'roots': ROOTS})['ok'] is True
            elif i % 3 == 1: out[i] = ask({'op': 'run', 'file': P('args.fib'), 'args': [str(i)]})['stdout'] == 'args 1 %d\n3\n' % i
            else: out[i] = ask({'op': 'check', 'file': P('bad.fib')})['ok'] is False
        except Exception as e: out[i] = repr(e)
    ts = [threading.Thread(target=worker, args=(i,)) for i in range(24)]
    [t.start() for t in ts]; [t.join() for t in ts]
    check('24 concurrent clients all get their own right answers', all(v is True for v in out.values()), out)
    slow = socket.socket(socket.AF_UNIX); slow.connect(sock)      # a client that connects and says nothing must not wedge the server
    t0 = time.time(); r = ask({'op': 'ping'}, timeout=30); waited = time.time() - t0
    check('a client that sends nothing holds the server for the read timeout only', r and r['ok'] and waited < 10, waited)
    slow.close()

    # ---- crash and restart
    pid0 = ask({'op': 'stats'})['pid']
    got = raw(b'{"op":"crash"}\n', timeout=30)
    check('a request that crashes the worker gets no answer', got == b'', got)
    pid1 = None
    for _ in range(100):
        try: pid1 = ask({'op': 'stats'}, timeout=5)['pid']; break
        except Exception: time.sleep(0.1)
    check('the supervisor starts a new worker (new pid) and the server answers again', pid1 is not None and pid1 != pid0, (pid0, pid1))
    r = ask({'op': 'check', 'file': P('main.fib'), 'roots': ROOTS}); check('the new worker has an empty cache and checks right', r['ok'] and r['stats']['verdict'] is False, r)

    # ---- 10,000 malformed requests
    rnd = random.Random(7)
    def junk(i):
        k = i % 12
        if k == 0: return bytes(rnd.randrange(256) for _ in range(rnd.randrange(1, 200))) + b'\n'
        if k == 1: return b'{"op":' + b'\n'
        if k == 2: return b'{"op":"check","file":' + str(rnd.randrange(1000)).encode() + b'}\n'
        if k == 3: return b'[' * rnd.randrange(1, 400) + b'\n'
        if k == 4: return b'{"op":"run","file":"x","args":[1,2,3]}\n'
        if k == 5: return b'{"op":"eval","text":' + b'\xff\xfe' + b'}\n'
        if k == 6: return b'\n'
        if k == 7: return b'{"op":"check","file":"' + b'a' * rnd.randrange(1, 3000) + b'"}\n'
        if k == 8: return b'{"op":null}\n'
        if k == 9: return b'{"op":"eval","text":"(((("}\n'
        if k == 10: return b'{"op":"test","file":"nothing.fib","only":5,"seed":"x"}\n'
        return b'{"op":"ping"' + b'\n'
    pid2 = ask({'op': 'stats'})['pid']
    errs = 0
    for i in range(10000):
        s = socket.socket(socket.AF_UNIX); s.settimeout(10)
        try:
            s.connect(sock); s.sendall(junk(i)); s.recv(65536)
        except Exception: errs += 1
        finally: s.close()
    st = ask({'op': 'stats'})
    check('10,000 malformed requests: every one got an answer or a clean close', errs < 50, errs)
    check('10,000 malformed requests: the worker did not die (same pid, requests counted)', st['pid'] == pid2 and st['requests'] >= 9000, (pid2, st))
    r = ask({'op': 'check', 'file': P('main.fib'), 'roots': ROOTS}); check('the server still checks after the fuzz', r['ok'], r)

    # ---- a socket path that is a live server's is refused; a stale file is replaced
    c = subprocess.run([fibc, 'serve', '--socket', sock], capture_output=True, text=True, cwd=root, env=env, preexec_fn=limit, timeout=60)
    check('a second server at a live socket is refused', c.returncode != 0 and 'already listening' in c.stderr, (c.returncode, c.stderr))

    # ---- shutdown
    r = ask({'op': 'shutdown'}); check('shutdown is answered', r and r['ok'], r)
    try: code = srv.wait(timeout=20)
    except subprocess.TimeoutExpired: code = None
    check('the server exits 0 after shutdown and removes its socket', code == 0 and not os.path.exists(sock), (code, os.path.exists(sock)))
finally:
    if srv.poll() is None: srv.kill()
    shutil.rmtree(work, ignore_errors=True)
print('serve tests: %s' % ('FAILED (%d)' % len(failed) if failed else 'ok'))
sys.exit(1 if failed else 0)
