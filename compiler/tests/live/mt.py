#!/usr/bin/env python3
"""compiler/tests/live/mt.py FIBC [ROOT]: an input run on the attach thread while the program's own thread allocates (mt-app.fib;
docs/design/host-threads.md). The attach thread is a second thread running program code in the program's runtime, so the runtime must be in
its multi-threaded mode from the start of the session: in single-thread mode both threads pop and push the same unlocked free lists.
Ten inputs of `(churn 300000)`, each of which must answer ok with the right number, then the program must still be alive and its stderr
free of `fatal signal`. Exit 0 when every check held."""
import json, os, socket, subprocess, sys, tempfile, time
fibc = os.path.abspath(sys.argv[1]); root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib'))); env.pop('FIB_LIVE', None)
out = tempfile.NamedTemporaryFile(prefix='live-out-', delete=False); err = tempfile.NamedTemporaryFile(prefix='live-err-', delete=False)
p = subprocess.Popen([fibc, 'run', '--live', os.path.join(root, 'compiler/tests/live/mt-app.fib')], stdout=out, stderr=err, cwd=root, env=env)
sock = None
for _ in range(1800):
    if p.poll() is not None: break
    for line in open(err.name).read().splitlines():
        if line.startswith('fibc: live at '): sock = line[len('fibc: live at '):]
    if sock and os.path.exists(sock) and 'mt-app: started' in open(out.name).read(): break
    time.sleep(0.1)
failed = []
def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)
want = None
try:
    s = socket.socket(socket.AF_UNIX); s.settimeout(300); s.connect(sock); buf = b''
    for k in range(10):
        s.sendall((json.dumps(dict(op='eval', text='(churn 300000)')) + '\n').encode())
        while b'\n' not in buf:
            d = s.recv(1 << 20)
            if not d: break
            buf += d
        if b'\n' not in buf: check('input %d answered' % k, False, 'connection closed (the program is %s)' % ('alive' if p.poll() is None else 'gone, exit %s' % p.poll())); break
        line, buf = buf.split(b'\n', 1); r = json.loads(line)
        if want is None: want = r.get('value')
        check('input %d: ok and the same sum' % k, r.get('ok') and r.get('value') == want and want, r)
except Exception as e:
    check('session', False, repr(e))
check('the program is alive', p.poll() is None, 'exit %s' % p.poll())
check('no fatal signal on stderr', 'fatal signal' not in open(err.name).read(), open(err.name).read()[-300:])
p.terminate()
try: p.wait(10)
except Exception: p.kill()
for f in (out.name, err.name): os.unlink(f)
sys.exit(len(failed))
