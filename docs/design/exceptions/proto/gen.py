#!/usr/bin/env python3
"""Generates the chain micro benchmark: D functions f_0..f_{D-1}, each owning one
refcounted object (alloc, release) across a call to the next; the leaf may fail.
Modes: plain (abort, today), invoke (landing pads), result (hidden {i64,i8} result),
flag (global flag after each call), jmp (setjmp/longjmp at the catch; no cleanup).
usage: gen.py MODE ALLOC(0|1) DEPTH > out.ll"""
import sys
mode, alloc, D = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
out = []
w = out.append
w('target triple = "x86_64-pc-linux-gnu"')
w('%R = type { i64, i8 }')
w('declare ptr @fib_alloc()')
w('declare void @fib_release(ptr)')
w('declare void @do_abort() noreturn')
w('declare void @do_throw() noreturn')
w('declare void @do_longjmp() noreturn')
w('declare i32 @__gxx_personality_v0(...)')
w('@thrown = external global i8')
RT = '%R' if mode == 'result' else 'i64'
PERS = ' personality ptr @__gxx_personality_v0' if (mode == 'invoke' and alloc) else ''
for k in range(D):
    leaf = (k == D - 1)
    w(f'define {RT} @f_{k}(i64 %x, i64 %t) noinline{PERS} {{')
    w('entry:')
    if alloc:
        w('  %o = call ptr @fib_alloc()')
    w(f'  %x1 = add i64 %x, {k + 3}')
    if leaf:
        w('  %c = icmp ne i64 %t, 0')
        w('  br i1 %c, label %thr, label %ok')
        w('thr:')
        if mode == 'plain':
            w('  call void @do_abort()'); w('  unreachable')
        elif mode == 'invoke':
            if alloc:
                w('  invoke void @do_throw() to label %unr unwind label %lpad')
                w('unr:'); w('  unreachable')
            else:
                w('  call void @do_throw()'); w('  unreachable')
        elif mode == 'jmp':
            w('  call void @do_longjmp()'); w('  unreachable')
        elif mode == 'result':
            if alloc: w('  call void @fib_release(ptr %o)')
            w('  ret %R { i64 0, i8 1 }')
        elif mode == 'flag':
            w('  store i8 1, ptr @thrown')
            if alloc: w('  call void @fib_release(ptr %o)')
            w('  ret i64 0')
        w('ok:')
        y = '%x1'
    else:
        nxt = f'@f_{k + 1}(i64 %x1, i64 %t)'
        if mode == 'invoke' and alloc:
            w(f'  %y = invoke i64 {nxt} to label %ok unwind label %lpad')
            w('ok:')
        elif mode == 'result':
            w(f'  %rr = call %R {nxt}')
            w('  %e = extractvalue %R %rr, 1')
            w('  %c = icmp ne i8 %e, 0')
            w('  br i1 %c, label %err, label %ok')
            w('err:')
            if alloc: w('  call void @fib_release(ptr %o)')
            w('  ret %R { i64 0, i8 1 }')
            w('ok:')
            w('  %y = extractvalue %R %rr, 0')
        elif mode == 'flag':
            w(f'  %y = call i64 {nxt}')
            w('  %f = load i8, ptr @thrown')
            w('  %c = icmp ne i8 %f, 0')
            w('  br i1 %c, label %err, label %ok')
            w('err:')
            if alloc: w('  call void @fib_release(ptr %o)')
            w('  ret i64 0')
            w('ok:')
        else:
            w(f'  %y = call i64 {nxt}')
        y = '%y'
    if alloc:
        w('  call void @fib_release(ptr %o)')
    w(f'  %r = add i64 {y}, 1')
    if mode == 'result':
        w('  %r1 = insertvalue %R undef, i64 %r, 0')
        w('  %r2 = insertvalue %R %r1, i8 0, 1')
        w('  ret %R %r2')
    else:
        w('  ret i64 %r')
    if mode == 'invoke' and alloc:
        w('lpad:')
        w('  %lp = landingpad { ptr, i32 } cleanup')
        w('  call void @fib_release(ptr %o)')
        w('  resume { ptr, i32 } %lp')
    w('}')
print('\n'.join(out))
