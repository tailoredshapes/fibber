#!/usr/bin/env python3
"""patch.py IN.lir OUT.lir : step 1 prototype. A trap on a spawned task's thread completes the
task (waking its joiners), releases the thread's count and ends only that thread."""
import sys
s = open(sys.argv[1]).read()
def sub(old, new, count=1):
    global s
    assert s.count(old) >= 1, old
    s = s.replace(old, new, count)
sub('(GLOBAL INTERNAL fib.tracing I32 (I32 0))',
    '(GLOBAL INTERNAL fib.tracing I32 (I32 0))\n(GLOBAL INTERNAL fib.task-key I32 (I32 0))\n'
    '(DECLARE pthread_key_create I32 (PTR PTR))\n(DECLARE pthread_setspecific I32 (I32 PTR))\n'
    '(DECLARE pthread_getspecific PTR (I32))\n(DECLARE pthread_exit VOID (PTR))')
sub('    (CALL (GLOBAL fib.rq-init))', '    (CALL (GLOBAL fib.rq-init))\n    (CALL (GLOBAL pthread_key_create) (GLOBAL fib.task-key) (PTR NULL))')
# every thread entry: remember the task
import re
s = re.sub(r'(\(DEFINE INTERNAL \(fib\.thread\.[^ ]+ PTR\) \(\(PTR task\)\)\n  \(BLOCK entry\n)',
           r'\1    (CALL (GLOBAL pthread_setspecific) (LOAD I32 (GLOBAL fib.task-key)) task)\n', s)
sub('''  (BLOCK entry
    (BR (ICMP NE (LOAD PTR (GLOBAL fibm.trap-hook)) (PTR NULL)) hooked plain))
  (BLOCK hooked
    (CALL (GLOBAL fib.trap-hooked) (PTR NULL) bytes n)
    (UNREACHABLE))
  (BLOCK plain''', '''  (BLOCK entry
    (BR (ICMP NE (LOAD PTR (GLOBAL fibm.trap-hook)) (PTR NULL)) hooked check))
  (BLOCK hooked
    (CALL (GLOBAL fib.trap-hooked) (PTR NULL) bytes n)
    (UNREACHABLE))
  (BLOCK check
    (LET ((tk (CALL (GLOBAL pthread_getspecific) (LOAD I32 (GLOBAL fib.task-key)))))
      (BR (ICMP NE tk (PTR NULL)) in-task plain)))
  (BLOCK in-task
    (LET ((tk2 (CALL (GLOBAL pthread_getspecific) (LOAD I32 (GLOBAL fib.task-key)))))
      (CALL (GLOBAL write) (I32 2) (STRING "trap in task: ") (I64 14))
      (CALL (GLOBAL write) (I32 2) bytes n)
      (CALL (GLOBAL write) (I32 2) (STRING "\\n") (I64 1))
      (CALL (GLOBAL fib.task-complete) tk2)
      (CALL (GLOBAL fib.release) tk2)
      (CALL (GLOBAL pthread_exit) (PTR NULL))
      (UNREACHABLE)))
  (BLOCK plain''')
open(sys.argv[2], 'w').write(s)
