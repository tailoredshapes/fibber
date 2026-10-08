; Like min-tail.ll with a local frame, so the epilogue has an `add sp` to compare:
; the incoming stack arguments are 16 bytes (the 9th and 10th i64), the frame is 16 bytes.
target triple = "aarch64-unknown-linux-gnu"

declare void @callee(ptr)

define tailcc void @f(i64 %a, i64 %b, i64 %c, i64 %d, i64 %e, i64 %f, i64 %g, i64 %h, i64 %i, i64 %j) {
  %x = alloca [4 x i64]
  store volatile i64 %j, ptr %x
  tail call void @callee(ptr null)
  ret void
}
