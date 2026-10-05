#!/usr/bin/env python3
"""patch.py IN.lir OUT.lir : step 1 prototype. A trap on a spawned task's thread completes the
task (waking its joiners), releases the thread's count and ends only that thread."""
import sys
s = open(sys.argv[1]).read()
def sub(old, new, count=1):
    global s
    assert s.count(old) >= 1, old
    s = s.replace(old, new, count)
sub('(global internal fib.tracing i32 (i32 0))',
    '(global internal fib.tracing i32 (i32 0))\n(global internal fib.task-key i32 (i32 0))\n'
    '(declare pthread_key_create i32 (ptr ptr))\n(declare pthread_setspecific i32 (i32 ptr))\n'
    '(declare pthread_getspecific ptr (i32))\n(declare pthread_exit void (ptr))')
sub('    (call @fib.rq-init)', '    (call @fib.rq-init)\n    (call @pthread_key_create @fib.task-key (ptr null))')
# every thread entry: remember the task
import re
s = re.sub(r'(\(define internal \(fib\.thread\.[^ ]+ ptr\) \(\(ptr task\)\)\n  \(block entry\n)',
           r'\1    (call @pthread_setspecific (load i32 @fib.task-key) task)\n', s)
sub('''  (block entry
    (br (icmp ne (load ptr @fibm.trap-hook) (ptr null)) hooked plain))
  (block hooked
    (call @fib.trap-hooked (ptr null) bytes n)
    (unreachable))
  (block plain''', '''  (block entry
    (br (icmp ne (load ptr @fibm.trap-hook) (ptr null)) hooked check))
  (block hooked
    (call @fib.trap-hooked (ptr null) bytes n)
    (unreachable))
  (block check
    (let ((tk (call @pthread_getspecific (load i32 @fib.task-key))))
      (br (icmp ne tk (ptr null)) in-task plain)))
  (block in-task
    (let ((tk2 (call @pthread_getspecific (load i32 @fib.task-key))))
      (call @write (i32 2) (string "trap in task: ") (i64 14))
      (call @write (i32 2) bytes n)
      (call @write (i32 2) (string "\\n") (i64 1))
      (call @fib.task-complete tk2)
      (call @fib.release tk2)
      (call @pthread_exit (ptr null))
      (unreachable)))
  (block plain''')
open(sys.argv[2], 'w').write(s)
