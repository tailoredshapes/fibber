; Minimal reproducer: an AArch64 tailcc function with two stack-passed arguments
; ends in an UNMARKED call of a ccc function (no `tail`, no `musttail`).
target triple = "aarch64-unknown-linux-gnu"

@sink = global i64 0

declare void @callee(i32)

define tailcc void @f(i64 %a, i64 %b, i64 %c, i64 %d, i64 %e, i64 %f, i64 %g, i64 %h, i64 %i, i64 %j) {
  %s = add i64 %a, %j
  store volatile i64 %s, ptr @sink
  tail call void @callee(i32 7)
  ret void
}
