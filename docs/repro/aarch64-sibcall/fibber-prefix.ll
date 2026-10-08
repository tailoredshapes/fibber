; lair -O0 output for cases/lir/instr/stackargs-sibcall.lir (aarch64-unknown-linux-gnu), with the `notail` mark removed: what 0.1.12 emitted.
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "aarch64-unknown-linux-gnu"

@sink = internal global i64 0
@fsink = internal global double 0.000000e+00
@popslot = global ptr @pop
@fpopslot = global ptr @fpop
@.str = private unnamed_addr constant [16 x i8] c"stackargs %lld\0A\00"
@.str.1 = private unnamed_addr constant [17 x i8] c"stackargs-fp %g\0A\00"

declare i32 @printf(ptr, ...)

declare void @srand(i32)

define tailcc void @pop(i64 %a, i64 %b, i64 %c, i64 %d, i64 %e, i64 %f, i64 %g, i64 %h, i64 %i, i64 %j) {
entry:
  %0 = add i64 %a, %j
  %1 = add i64 %b, %c
  %2 = add i64 %d, %e
  %3 = add i64 %h, %i
  %4 = add i64 %g, %3
  %5 = add i64 %f, %4
  %6 = add i64 %2, %5
  %7 = add i64 %1, %6
  %8 = add i64 %0, %7
  store i64 %8, ptr @sink, align 8
  call void @srand(i32 7)
  ret void
}

define tailcc void @fpop(double %a, double %b, double %c, double %d, double %e, double %f, double %g, double %h, double %i, double %j) {
entry:
  %0 = fadd double %a, %j
  %1 = fadd double %b, %c
  %2 = fadd double %d, %e
  %3 = fadd double %h, %i
  %4 = fadd double %g, %3
  %5 = fadd double %f, %4
  %6 = fadd double %2, %5
  %7 = fadd double %1, %6
  %8 = fadd double %0, %7
  store double %8, ptr @fsink, align 8
  call void @srand(i32 7)
  ret void
}

define tailcc i64 @drive(i64 %i, i64 %n, i64 %acc) {
entry:
  %0 = icmp eq i64 %i, %n
  br i1 %0, label %done, label %more

done:                                             ; preds = %entry
  ret i64 %acc

more:                                             ; preds = %entry
  %1 = load ptr, ptr @popslot, align 8
  call tailcc void %1(i64 %i, i64 0, i64 0, i64 0, i64 0, i64 0, i64 0, i64 0, i64 0, i64 10)
  %2 = add i64 %i, 1
  %3 = load i64, ptr @sink, align 8
  %4 = add i64 %acc, %3
  %5 = musttail call tailcc i64 @drive(i64 %2, i64 %n, i64 %4)
  ret i64 %5
}

define tailcc double @fdrive(i64 %i, i64 %n, double %acc) {
entry:
  %0 = icmp eq i64 %i, %n
  br i1 %0, label %done, label %more

done:                                             ; preds = %entry
  ret double %acc

more:                                             ; preds = %entry
  %1 = load ptr, ptr @fpopslot, align 8
  call tailcc void %1(double 5.000000e-01, double 0.000000e+00, double 0.000000e+00, double 0.000000e+00, double 0.000000e+00, double 0.000000e+00, double 0.000000e+00, double 0.000000e+00, double 0.000000e+00, double 0.000000e+00)
  %2 = add i64 %i, 1
  %3 = load double, ptr @fsink, align 8
  %4 = fadd double %acc, %3
  %5 = musttail call tailcc double @fdrive(i64 %2, i64 %n, double %4)
  ret double %5
}

define i32 @main() {
entry:
  %0 = call tailcc i64 @drive(i64 0, i64 1000, i64 0)
  %1 = call i32 (ptr, ...) @printf(ptr @.str, i64 %0)
  %2 = call tailcc double @fdrive(i64 0, i64 41, double 0.000000e+00)
  %3 = call i32 (ptr, ...) @printf(ptr @.str.1, double %2)
  ret i32 0
}
