; ModuleID = 't3.ll'
source_filename = "t3.ll"
target datalayout = "e-p6:32:32-i64:64-i128:128-v16:16-v32:32-n16:32:64"
target triple = "nvptx64-nvidia-cuda"

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: readwrite)
define ptx_kernel void @vadd(ptr readonly captures(none) %p0, ptr readonly captures(none) %p1, ptr writeonly captures(none) %p2, i64 %p3) local_unnamed_addr #0 {
entry:
  %t1 = tail call i32 @llvm.nvvm.read.ptx.sreg.ctaid.x()
  %t8 = zext nneg i32 %t1 to i64
  %t9 = tail call i32 @llvm.nvvm.read.ptx.sreg.ntid.x()
  %t16 = zext nneg i32 %t9 to i64
  %t17 = tail call i32 @llvm.nvvm.read.ptx.sreg.tid.x()
  %t24 = zext nneg i32 %t17 to i64
  %t25 = mul nuw nsw i64 %t8, %t16
  %t26 = add nuw nsw i64 %t25, %t24
  %t29 = icmp slt i64 %t26, %p3
  br i1 %t29, label %then30, label %join32

then30:                                           ; preds = %entry
  %t1.i = shl nuw nsw i64 %t26, 2
  %t2.i5 = getelementptr i8, ptr %p2, i64 %t1.i
  %t2.i2 = getelementptr i8, ptr %p1, i64 %t1.i
  %t2.i = getelementptr i8, ptr %p0, i64 %t1.i
  %t3.i = load float, ptr %t2.i, align 4
  %t3.i3 = load float, ptr %t2.i2, align 4
  %t35 = fadd float %t3.i, %t3.i3
  store float %t35, ptr %t2.i5, align 4
  br label %join32

join32:                                           ; preds = %entry, %then30
  ret void
}

; Function Attrs: mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare noundef range(i32 0, 2147483647) i32 @llvm.nvvm.read.ptx.sreg.ctaid.x() #1

; Function Attrs: mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare noundef range(i32 1, 1025) i32 @llvm.nvvm.read.ptx.sreg.ntid.x() #1

; Function Attrs: mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare noundef range(i32 0, 1024) i32 @llvm.nvvm.read.ptx.sreg.tid.x() #1

attributes #0 = { mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: readwrite) }
attributes #1 = { mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none) }
