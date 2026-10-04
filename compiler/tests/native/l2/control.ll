; ModuleID = 'compiler/tests/native/l2/control.lir'
source_filename = "compiler/tests/native/l2/control.lir"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

define i64 @sum(i64 %n) {
entry:
  br label %loop

loop:                                             ; preds = %loop, %entry
  %i = phi i64 [ 1, %entry ], [ %i2, %loop ]
  %acc = phi i64 [ 0, %entry ], [ %acc2, %loop ]
  %acc2 = add i64 %acc, %i
  %i2 = add i64 %i, 1
  %0 = icmp sle i64 %i2, %n
  br i1 %0, label %loop, label %exit

exit:                                             ; preds = %loop
  ret i64 %acc2
}

define i32 @sw(i32 %k) {
entry:
  switch i32 %k, label %other [
    i32 0, label %zero
    i32 1, label %one
    i32 -5, label %neg
  ]

zero:                                             ; preds = %entry
  ret i32 100

one:                                              ; preds = %entry
  ret i32 200

neg:                                              ; preds = %entry
  ret i32 300

other:                                            ; preds = %entry
  ret i32 -1
}

define i32 @both(i1 %c) {
entry:
  br i1 %c, label %j, label %j

j:                                                ; preds = %entry, %entry
  %0 = phi i32 [ 4, %entry ], [ 4, %entry ]
  ret i32 %0
}

define i64 @count(i64 %n, i64 %acc) {
entry:
  %0 = icmp eq i64 %n, 0
  br i1 %0, label %done, label %more

done:                                             ; preds = %entry
  ret i64 %acc

more:                                             ; preds = %entry
  %1 = sub i64 %n, 1
  %2 = add i64 %acc, 1
  %3 = musttail call i64 @count(i64 %1, i64 %2)
  ret i64 %3
}

define tailcc i64 @step(ptr %self, i64 %n) {
entry:
  %0 = musttail call tailcc i64 %self(ptr %self, i64 %n)
  ret i64 %0
}

define void @nothing() {
entry:
  call void @llvm.trap()
  call void @nothing()
  unreachable
}

; Function Attrs: cold noreturn nounwind memory(inaccessiblemem: write)
declare void @llvm.trap() #0

attributes #0 = { cold noreturn nounwind memory(inaccessiblemem: write) }
