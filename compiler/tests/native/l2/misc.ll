; ModuleID = 'compiler/tests/native/l2/misc.lir'
source_filename = "compiler/tests/native/l2/misc.lir"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

@.str = private unnamed_addr constant [3 x i8] c"hi\00"

declare i32 @puts(ptr)

define i64 @m(double %d, <4 x i32> %v, i64 %w) {
entry:
  %n = fneg double %d
  %c = fcmp olt double %d, %n
  %p = call i64 @llvm.ctpop.i64(i64 %w)
  %o = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %w, i64 %w)
  %s = call i64 @llvm.fptosi.sat.i64.f64(double %d)
  %u = call i8 @llvm.fptoui.sat.i8.f64(double %d)
  %e = extractelement <4 x i32> %v, i32 0
  %iv = insertelement <4 x i32> %v, i32 %e, i32 1
  %sh = shufflevector <4 x i32> %v, <4 x i32> %iv, <4 x i32> <i32 3, i32 7, i32 0, i32 4>
  %z = zext i32 %e to i64
  %0 = insertvalue { i64, i8 } poison, i64 %w, 0
  %st = insertvalue { i64, i8 } %0, i8 1, 1
  %1 = insertvalue [2 x i64] poison, i64 %w, 0
  %ar = insertvalue [2 x i64] %1, i64 %z, 1
  %2 = extractvalue { i64, i8 } %st, 1
  %iv2 = insertvalue { i64, i8 } %st, i8 5, 1
  %3 = extractvalue [2 x i64] %ar, 1
  %ar2 = insertvalue [2 x i64] %ar, i64 %w, 1
  %4 = extractvalue { i64, i1 } %o, 0
  %5 = extractvalue [2 x i64] %ar2, 1
  %k = select i1 %c, i64 %4, i64 %5
  %t = call i32 @puts(ptr @.str)
  %b = bitcast double %d to i64
  %tr = trunc i64 %w to i8
  %f = sitofp i64 %w to double
  %pp = inttoptr i64 %w to ptr
  %q = ptrtoint ptr %pp to i64
  %6 = extractvalue { i64, i8 } %iv2, 0
  %7 = zext i8 %u to i64
  %8 = add i64 %7, %q
  %9 = add i64 %b, %8
  %10 = add i64 %z, %9
  %11 = add i64 0, %10
  %12 = add i64 %6, %11
  %13 = add i64 %k, %12
  ret i64 %13
}

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i64 @llvm.ctpop.i64(i64) #0

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64) #0

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i64 @llvm.fptosi.sat.i64.f64(double) #0

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i8 @llvm.fptoui.sat.i8.f64(double) #0

attributes #0 = { nocallback nofree nosync nounwind speculatable willreturn memory(none) }
