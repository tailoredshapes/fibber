; ModuleID = 'compiler/tests/native/l2/memory.lir'
source_filename = "compiler/tests/native/l2/memory.lir"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

define i64 @g(ptr %p, i64 %n) {
entry:
  %a = alloca i64, align 16
  %q = getelementptr inbounds i64, ptr %p, i64 %n
  store i64 7, ptr %a, align 8
  store volatile i64 %n, ptr %q, align 8
  store atomic i64 %n, ptr %p seq_cst, align 8
  %l = load atomic i64, ptr %p acquire, align 8
  %r = atomicrmw add ptr %p, i64 %l seq_cst, align 8
  %c = cmpxchg weak ptr %p, i64 %r, i64 %n seq_cst monotonic, align 8
  fence seq_cst
  %0 = load i64, ptr %a, align 8
  %1 = extractvalue { i64, i1 } %c, 0
  %2 = add i64 %0, %1
  ret i64 %2
}
