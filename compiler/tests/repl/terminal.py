#!/usr/bin/env python3
"""compiler/tests/repl/terminal.py FIBC: the REPL on a terminal (docs/design/dev-loop.md section 9.3): raw mode, line editing, history, multi-line continuation, ctrl-c, ctrl-d.
It runs `fibc repl` on a pseudo-terminal, types keys and looks for the results in what the terminal shows. Exit 0 when every check held."""
import os, pty, select, sys, time, tempfile, shutil, resource

fibc = os.path.abspath(sys.argv[1])
root = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', '..'))
home = tempfile.mkdtemp(prefix='repl-tty-')
failed = []

def run(keys, waits):
    """Starts the REPL, sends each key sequence in turn and waits for `waits[i]` to show; returns (all output, exit status)."""
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(root)
        resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
        os.environ['HOME'] = home
        os.environ['FIB_LIB'] = os.environ.get('FIB_LIB', os.path.join(root, 'lib'))
        os.execv(fibc, [fibc, 'repl'])
    out = b''
    def read_until(token, secs=300):
        nonlocal out
        end = time.time() + secs
        while token.encode() not in out and time.time() < end:
            r, _, _ = select.select([fd], [], [], 0.2)
            if r:
                try: d = os.read(fd, 65536)
                except OSError: break
                if not d: break
                out += d
    read_until('fib> ')
    for k, w in zip(keys, waits):
        before = out.count('fib> '.encode()) + out.count('...  '.encode())
        os.write(fd, k.encode())
        if '\r' in k or w == 'prompt':    # an input line was sent: wait for the next prompt (the REPL is in cooked mode while a program runs)
            end = time.time() + 300
            while out.count('fib> '.encode()) + out.count('...  '.encode()) <= before and time.time() < end: read_until('\0', 0.3)
        time.sleep(0.05)
    deadline = time.time() + 120; status = None
    while time.time() < deadline:
        p, st = os.waitpid(pid, os.WNOHANG)
        if p:
            status = os.waitstatus_to_exitcode(st)
            for _ in range(20):    # what the process wrote before it ended is still in the terminal's buffer
                r, _, _ = select.select([fd], [], [], 0.05)
                if not r: break
                try: d = os.read(fd, 65536)
                except OSError: break
                if not d: break
                out += d
            break
        r, _, _ = select.select([fd], [], [], 0.1)
        if r:
            try: out += os.read(fd, 65536)
            except OSError: pass
    if status is None: os.kill(pid, 9); os.waitpid(pid, 0)
    return out.decode('utf-8', 'replace'), status

def check(name, cond, why=''):
    if cond: print('ok', name)
    else: print('FAIL %s: %s' % (name, why)); failed.append(name)

try:
    # typed, edited with backspace and the arrows, history recalled with up, a continuation line, ctrl-c, ctrl-d
    out, st = run(['(+ 1 2\r', ')\r', '(+ 10 5x\x7f)\r', '\x1b[A', '\r', '(* 2 \x01\x0b(+ 20 22)\r', '(+ 1\x03', '\x04'],
                  [None, '3', '15', None, None, '42', None, None])
    check('a line is typed and its value printed', '\r\n3\r\n' in out, repr(out[-300:]))
    check('an open bracket continues the input on the next line', '...  ' in out, repr(out[-300:]))
    check('backspace edits the line', '\r\n15\r\n' in out, repr(out[-400:]))
    check('up recalls the previous input (it runs again)', out.count('\r\n15\r\n') >= 2, repr(out[-400:]))
    check('ctrl-a and ctrl-k edit the line (the typed text before the cursor is dropped)', '\r\n42\r\n' in out, repr(out[-400:]))
    check('ctrl-c drops the line', '^C' in out, repr(out[-300:]))
    check('ctrl-d on an empty line ends the session with status 0', st == 0, st)
    hist = os.path.join(home, '.fibc_history')
    check('the history is written to ~/.fibc_history', os.path.exists(hist) and '(+ 10 5)' in open(hist).read().replace('\n', ' '), os.path.exists(hist) and open(hist).read())
    # a trap in the evaluated program returns to the prompt on a terminal too
    out, st = run(['(nth [1] 3)\r', '(+ 4 4)\r', '\x04'], ['trapped', '8', None])
    check('a trap returns to the prompt and the next input runs', 'the program trapped' in out and '\r\n8\r\n' in out and st == 0, (st, repr(out[-300:])))
finally:
    shutil.rmtree(home, ignore_errors=True)
print('repl tty tests: %s' % ('FAILED (%d)' % len(failed) if failed else 'ok'))
sys.exit(1 if failed else 0)
