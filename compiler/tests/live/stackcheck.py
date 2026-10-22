#!/usr/bin/env python3
"""compiler/tests/live/stackcheck.py FIBC [ROOT]: a recursion an input starts past the stack of the attach thread (stackcheck-app.fib; docs/design/stack-check.md).
Before the stack check the process ended (`trap: stack overflow`, status 134, the connection closed with no answer). Now each of these must hold:
  (up 1000) and (up 100000) answer (a depth that fits is not reduced);
  (up 100000000) is an error reply whose text says `stack overflow`, and the session answers the next input;
  (grow 100000000), which allocates a Vec and a str in every frame, is the same error, and allocation and free work after it (churn, grow 1000, 10 more rounds);
  the program is alive at the end and its standard error has no `trap:` and no `fatal signal`.
FIB_STACK_CHECK=0 in the environment plants the fault (no check is emitted): the session dies and the checks fail. Exit 0 when every check held."""
import json, os, socket, subprocess, sys, tempfile, time
fibc = os.path.abspath(sys.argv[1]); root = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.getcwd()
runtime = tempfile.mkdtemp(prefix='fl-', dir='/tmp')
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, FIB_LIB=os.environ.get('FIB_LIB', os.path.join(root, 'lib'))); env.pop('FIB_LIVE', None)
out = tempfile.NamedTemporaryFile(prefix='live-out-', delete=False); err = tempfile.NamedTemporaryFile(prefix='live-err-', delete=False)
p = subprocess.Popen([fibc, 'run', '--live', os.path.join(root, 'compiler/tests/live/stackcheck-app.fib')], stdout=out, stderr=err, cwd=root, env=env)
sock = None
for _ in range(1800):
    if p.poll() is not None: break
    for line in open(err.name).read().splitlines():
        if line.startswith('fibc: live at '): sock = line[len('fibc: live at '):]
    if sock and os.path.exists(sock) and 'stackcheck-app: started' in open(out.name).read(): break
    time.sleep(0.1)
failed = []
def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)
buf = b''
def ask(s, text):
    global buf
    s.sendall((json.dumps(dict(op='eval', text=text)) + '\n').encode())
    while b'\n' not in buf:
        d = s.recv(1 << 20)
        if not d: return None
        buf += d
    line, buf = buf.split(b'\n', 1)
    return json.loads(line)
try:
    s = socket.socket(socket.AF_UNIX); s.settimeout(300); s.connect(sock)
    r = ask(s, '(up 1000)');   check('a shallow recursion answers', r and r.get('ok') and r.get('value') == '1000', r)
    r = ask(s, '(up 100000)'); check('100,000 frames answer', r and r.get('ok') and r.get('value') == '100000', r)
    for k in range(3):
        r = ask(s, '(up 100000000)')
        check('overflow %d is an error reply that says stack overflow' % k, r is not None and r.get('ok') is False and 'stack overflow' in r.get('failed', ''),
              r if r is not None else 'connection closed (the program is %s)' % ('alive' if p.poll() is None else 'gone, exit %s' % p.poll()))
        if r is None: break
        r = ask(s, '(+ 1 2)'); check('the next input answers (%d)' % k, r and r.get('ok') and r.get('value') == '3', r)
    r = ask(s, '(grow 100000000)')
    check('an allocating recursion overflows the same way', r is not None and r.get('ok') is False and 'stack overflow' in r.get('failed', ''), r)
    r = ask(s, '(grow 1000)'); check('allocation and free work after it', r and r.get('ok') and r.get('value') == '7893', r)
    for k in range(10):
        r = ask(s, '(churn 100000)')
        if not (r and r.get('ok')): check('churn %d after the overflows' % k, False, r); break
    else: check('ten churns of 100000 allocations after the overflows', True)
    for k in range(5):
        r = ask(s, '(grow 100000000)')
        if not (r and r.get('ok') is False and 'stack overflow' in r.get('failed', '')): check('allocating overflow %d again' % k, False, r); break
        r = ask(s, '(grow 200)')
        if not (r and r.get('ok')): check('grow after overflow %d' % k, False, r); break
    else: check('five more allocating overflows, each followed by an answer', True)
except Exception as e:
    check('session', False, repr(e))
check('the program is alive', p.poll() is None, 'exit %s' % p.poll())
errtext = open(err.name).read()
check('no trap or fatal signal on stderr', 'trap:' not in errtext and 'fatal signal' not in errtext, errtext[-300:])
p.terminate()
try: p.wait(10)
except Exception: p.kill()
for f in (out.name, err.name): os.unlink(f)
sys.exit(len(failed))
