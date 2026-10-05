; unwinding through JIT-compiled frames: main catches what thrower throws through mid (which has a cleanup)
declare ptr @__cxa_allocate_exception(i64)
declare void @__cxa_throw(ptr, ptr, ptr) noreturn
declare ptr @__cxa_begin_catch(ptr)
declare void @__cxa_end_catch()
declare i32 @__gxx_personality_v0(...)
declare i32 @puts(ptr)
@_ZTIi = external constant ptr
@s1 = private constant [13 x i8] c"mid cleanup\0A\00"
define void @thrower() {
  %e = call ptr @__cxa_allocate_exception(i64 4)
  store i32 42, ptr %e
  call void @__cxa_throw(ptr %e, ptr @_ZTIi, ptr null)
  unreachable
}
define i32 @mid() personality ptr @__gxx_personality_v0 {
  invoke void @thrower() to label %ok unwind label %lp
ok:
  ret i32 1
lp:
  %l = landingpad { ptr, i32 } cleanup
  call i32 @puts(ptr @s1)
  resume { ptr, i32 } %l
}
define i32 @main() personality ptr @__gxx_personality_v0 {
  %r = invoke i32 @mid() to label %ok unwind label %lp
ok:
  ret i32 %r
lp:
  %l = landingpad { ptr, i32 } catch ptr @_ZTIi
  %p = extractvalue { ptr, i32 } %l, 0
  call ptr @__cxa_begin_catch(ptr %p)
  call void @__cxa_end_catch()
  ret i32 7
}
