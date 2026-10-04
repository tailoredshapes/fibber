; ModuleID = 'compiler/tests/native/l2/arith.lir'
source_filename = "compiler/tests/native/l2/arith.lir"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

define i64 @f(i64 %x, i64 %y) {
entry:
  %a = add i64 %x, %y
  %c = icmp slt i64 %a, %y
  %s = select i1 %c, i64 %a, i64 %y
  ret i64 %s
}
