# Builtin signatures

Generated from the maintained [stage-2 table](../../compiler/types/builtins.fib). This lists signatures, bounds and unsafe requirements, not all checker special forms. Stability classes and “since” versions have not been approved or recorded per export; they are deliberately not inferred.

| Name | Signature | Bounds | Unsafe only |
|---|---|---|---|
| `cell` | `(fn (a) (Cell a))` | `` | false |
| `set!` | `(fn ((Cell a) a) unit)` | `` | false |
| `atom` | `(fn (a) (Atom a))` | `((Send a))` | false |
| `swap!` | `(fn ((Atom a) (fn (a) a)) a)` | `` | false |
| `reset!` | `(fn ((Atom a) a) unit)` | `` | false |
| `compare-and-set!` | `(fn ((Atom a) a a) bool)` | `` | false |
| `freeze` | `(fn (a) a)` | `((Object a) (Send a))` | false |
| `private-copy` | `(fn (a) a)` | `((Object a) (Send a))` | false |
| `frozen?` | `(fn (a) bool)` | `((Object a))` | false |
| `weak` | `(fn (a) (Weak a))` | `((Weakable a))` | false |
| `spawn` | `(fn ((fn :send () a)) (Task a))` | `((Send a))` | false |
| `fork-task` | `(fn ((fn :send () a)) (Task a))` | `((Send a))` | false |
| `join` | `(fn ((Task a)) a)` | `` | false |
| `task-failure` | `(fn ((Task a)) (Option str))` | `` | false |
| `trap` | `(fn (str) a)` | `` | false |
| `not` | `(fn (bool) bool)` | `` | false |
| `array` | `(fn (i64 a) (Array a))` | `` | false |
| `array-len` | `(fn ((Array a)) i64)` | `` | false |
| `array-get` | `(fn ((Array a) i64) a)` | `` | false |
| `array-with` | `(fn ((Array a) i64 a) (Array a))` | `` | false |
| `array-copy` | `(fn ((Array a) i64 i64) (Array a))` | `` | false |
| `array-set!` | `(fn ((& (Array a)) i64 a) unit)` | `` | false |
| `array-blit!` | `(fn ((& (Array a)) i64 (Array a) i64 i64) unit)` | `` | false |
| `cell-update!` | `(fn ((Cell a) (fn :send (a) a)) unit)` | `` | false |
| `array-take!` | `(fn ((& (Array a)) i64) a)` | `` | false |
| `array-push!` | `(fn ((& (Array a)) a) unit)` | `` | false |
| `array-pop!` | `(fn ((& (Array a))) a)` | `` | false |
| `str-len` | `(fn (str) i64)` | `` | false |
| `str-bytes` | `(fn (str) (Array i8))` | `` | false |
| `str-from-bytes` | `(fn ((Array i8)) str)` | `` | false |
| `args` | `(fn () (Vec str))` | `` | false |
| `read-file` | `(fn (str) (Option str))` | `` | false |
| `write-file` | `(fn (str str) bool)` | `` | false |
| `sys-open` | `(fn (str i64 i64) i64)` | `` | false |
| `sys-close` | `(fn (i64) i64)` | `` | false |
| `sys-read` | `(fn (i64 i64) (Array i8))` | `` | false |
| `sys-write` | `(fn (i64 (Array i8) i64 i64) i64)` | `` | false |
| `sys-seek` | `(fn (i64 i64 i64) i64)` | `` | false |
| `sys-pipe` | `(fn () i64)` | `` | false |
| `sys-dup` | `(fn (i64) i64)` | `` | false |
| `sys-isatty` | `(fn (i64) bool)` | `` | false |
| `sys-unlink` | `(fn (str) i64)` | `` | false |
| `sys-mkdir` | `(fn (str i64) i64)` | `` | false |
| `sys-rmdir` | `(fn (str) i64)` | `` | false |
| `sys-errno-text` | `(fn (i64) str)` | `` | false |
| `sys-getenv` | `(fn (str) (Option str))` | `` | false |
| `sys-clock-now` | `(fn () i64)` | `` | false |
| `sys-thread-stripe` | `(fn () i64)` | `` | false |
| `sys-wall-now` | `(fn () i64)` | `` | false |
| `sys-sleep` | `(fn (i64) i64)` | `` | false |
| `str-concat` | `(fn (str str) str)` | `` | false |
| `str-slice` | `(fn (str i64 i64) str)` | `` | false |
| `str-byte-at` | `(fn (str i64) i8)` | `` | false |
| `str-find` | `(fn (str str i64) (Option i64))` | `` | false |
| `str-eq` | `(fn (str str) bool)` | `` | false |
| `str->keyword` | `(fn (str) keyword)` | `` | false |
| `starts-with?` | `(fn (str str) bool)` | `` | false |
| `char->i32` | `(fn (char) i32)` | `` | false |
| `i32->char` | `(fn (i32) char)` | `` | false |
| `f64->bits` | `(fn (f64) i64)` | `` | false |
| `bits->f64` | `(fn (i64) f64)` | `` | false |
| `f32->bits` | `(fn (f32) i32)` | `` | false |
| `bits->f32` | `(fn (i32) f32)` | `` | false |
| `concat` | `(fn ((Vec a) (Vec a)) (Vec a))` | `` | false |
| `gensym` | `(fn (str) Form)` | `` | false |
| `struct?` | `(fn (Form) bool)` | `` | false |
| `struct-fields` | `(fn (Form) (Vec Form))` | `` | false |
| `struct-params` | `(fn (Form) (Vec Form))` | `` | false |
| `struct-field-types` | `(fn (Form) (Vec Form))` | `` | false |
| `enum?` | `(fn (Form) bool)` | `` | false |
| `enum-params` | `(fn (Form) (Vec Form))` | `` | false |
| `enum-variants` | `(fn (Form) (Vec Form))` | `` | false |
| `ptr+` | `(fn (ptr i64) ptr)` | `` | true |
| `load-i8` | `(fn (ptr) i8)` | `` | true |
| `load-i16` | `(fn (ptr) i16)` | `` | true |
| `load-i32` | `(fn (ptr) i32)` | `` | true |
| `load-i64` | `(fn (ptr) i64)` | `` | true |
| `load-ptr` | `(fn (ptr) ptr)` | `` | true |
| `store-i8` | `(fn (ptr i8) unit)` | `` | true |
| `store-i16` | `(fn (ptr i16) unit)` | `` | true |
| `store-i32` | `(fn (ptr i32) unit)` | `` | true |
| `store-i64` | `(fn (ptr i64) unit)` | `` | true |
| `store-ptr` | `(fn (ptr ptr) unit)` | `` | true |
| `alloc` | `(fn (i64) ptr)` | `` | true |
| `free` | `(fn (ptr) unit)` | `` | true |
| `raw` | `(fn (a) ptr)` | `((Object a))` | true |
| `raw-retained` | `(fn (a) ptr)` | `((Object a))` | true |
| `release-raw` | `(fn (ptr) unit)` | `` | true |
| `simd/lt` | `(fn ((Simd t n) (Simd t n)) (Simd bool n))` | `((Num t))` | false |
| `simd/le` | `(fn ((Simd t n) (Simd t n)) (Simd bool n))` | `((Num t))` | false |
| `simd/gt` | `(fn ((Simd t n) (Simd t n)) (Simd bool n))` | `((Num t))` | false |
| `simd/ge` | `(fn ((Simd t n) (Simd t n)) (Simd bool n))` | `((Num t))` | false |
| `simd/eq` | `(fn ((Simd t n) (Simd t n)) (Simd bool n))` | `((Num t))` | false |
| `simd/ne` | `(fn ((Simd t n) (Simd t n)) (Simd bool n))` | `((Num t))` | false |
| `lane` | `(fn ((Simd t n) i64) t)` | `` | false |
| `with-lane` | `(fn ((Simd t n) i64 t) (Simd t n))` | `` | false |
| `hsum` | `(fn ((Simd t n)) t)` | `((Num t))` | false |
| `hmin` | `(fn ((Simd t n)) t)` | `((Num t))` | false |
| `hmax` | `(fn ((Simd t n)) t)` | `((Num t))` | false |
| `simd/and` | `(fn ((Simd bool n) (Simd bool n)) (Simd bool n))` | `` | false |
| `simd/or` | `(fn ((Simd bool n) (Simd bool n)) (Simd bool n))` | `` | false |
| `simd/xor` | `(fn ((Simd bool n) (Simd bool n)) (Simd bool n))` | `` | false |
| `simd/not` | `(fn ((Simd bool n)) (Simd bool n))` | `` | false |
| `simd/any` | `(fn ((Simd bool n)) bool)` | `` | false |
| `simd/all` | `(fn ((Simd bool n)) bool)` | `` | false |
| `simd/blend` | `(fn ((Simd bool n) (Simd t n) (Simd t n)) (Simd t n))` | `` | false |
| `unchecked-add` | `(fn (t t) t)` | `((Bits t))` | false |
| `unchecked-subtract` | `(fn (t t) t)` | `((Bits t))` | false |
| `unchecked-multiply` | `(fn (t t) t)` | `((Bits t))` | false |
| `unchecked-negate` | `(fn (t) t)` | `((Bits t))` | false |
| `load-f32` | `(fn (ptr) f32)` | `` | true |
| `load-f64` | `(fn (ptr) f64)` | `` | true |
| `store-f32` | `(fn (ptr f32) unit)` | `` | true |
| `store-f64` | `(fn (ptr f64) unit)` | `` | true |
| `load-f32x4` | `(fn (ptr) (Simd f32 4))` | `` | true |
| `load-f64x4` | `(fn (ptr) (Simd f64 4))` | `` | true |
| `store-f32x4` | `(fn (ptr (Simd f32 4)) unit)` | `` | true |
| `store-f64x4` | `(fn (ptr (Simd f64 4)) unit)` | `` | true |
| `array-uninit-f32` | `(fn (i64) (Array f32))` | `` | true |
| `array-uninit-f64` | `(fn (i64) (Array f64))` | `` | true |
| `array-uninit-i8` | `(fn (i64) (Array i8))` | `` | true |
| `load-f32x8` | `(fn (ptr) (Simd f32 8))` | `` | true |
| `store-f32x8` | `(fn (ptr (Simd f32 8)) unit)` | `` | true |
| `simd/lanes` | `(fn ((Simd t n)) i64)` | `` | false |
| `simd/kind` | `(fn ((Simd t n)) i64)` | `` | false |
| `simd/fma` | `(fn (t t t) t)` | `((Float t))` | false |
| `simd/sqrt` | `(fn (t) t)` | `((Float t))` | false |
| `simd/floor` | `(fn (t) t)` | `((Float t))` | false |
| `simd/ceil` | `(fn (t) t)` | `((Float t))` | false |
| `simd/trunc` | `(fn (t) t)` | `((Float t))` | false |
| `simd/round` | `(fn (t) t)` | `((Float t))` | false |
| `simd/round-even` | `(fn (t) t)` | `((Float t))` | false |
| `simd/abs` | `(fn (t) t)` | `((Num t))` | false |
| `simd/min` | `(fn (t t) t)` | `((Num t))` | false |
| `simd/max` | `(fn (t t) t)` | `((Num t))` | false |
| `simd/min-num` | `(fn (t t) t)` | `((Float t))` | false |
| `simd/max-num` | `(fn (t t) t)` | `((Float t))` | false |
| `simd/convert` | `(fn ((Simd s n)) (Simd t n))` | `((Num s) (Num t))` | false |
| `simd/reverse` | `(fn ((Simd t n)) (Simd t n))` | `` | false |
| `simd/shuffle` | `(fn ((Simd t n) (Simd t n) (Simd i64 m)) (Simd t m))` | `` | false |
| `simd-load` | `(fn ((Array t) i64) (Simd t n))` | `` | false |
| `simd-load-unchecked` | `(fn ((Array t) i64) (Simd t n))` | `` | true |
| `simd-load-tail` | `(fn ((Array t) i64) (Simd t n))` | `` | false |
| `simd-load-masked` | `(fn ((Array t) i64 (Simd bool n) (Simd t n)) (Simd t n))` | `` | false |
| `simd-store!` | `(fn ((& (Array t)) i64 (Simd t n)) unit)` | `` | false |
| `simd-store-unchecked!` | `(fn ((& (Array t)) i64 (Simd t n)) unit)` | `` | true |
| `simd-store-tail!` | `(fn ((& (Array t)) i64 (Simd t n)) unit)` | `` | false |
| `simd-store-masked!` | `(fn ((& (Array t)) i64 (Simd t n) (Simd bool n)) unit)` | `` | false |
| `simd/muladd` | `(fn (t t t) t)` | `((Float t))` | false |
| `simd/bitcast` | `(fn ((Simd s n)) (Simd t n))` | `((Num s) (Num t))` | false |
| `catch-run` | `(fn ((fn () unit)) i64)` | `` | false |
| `caught-message` | `(fn () str)` | `` | false |
| `caught-object` | `(fn () a)` | `((Object a))` | true |
| `throw-object` | `(fn (a) b)` | `((Object a))` | true |
| `catch-active?` | `(fn () bool)` | `` | false |
| `finally-enter` | `(fn () i64)` | `` | false |
| `finally-leave` | `(fn (i64) unit)` | `` | false |
| `load-simd` | `(fn (ptr) (Simd t n))` | `((Num t))` | true |
| `store-simd` | `(fn (ptr (Simd t n)) unit)` | `((Num t))` | true |
| `simd/movemask` | `(fn ((Simd bool n)) i64)` | `` | false |
| `ctz` | `(fn (t) t)` | `((Bits t))` | false |
| `clz` | `(fn (t) t)` | `((Bits t))` | false |
| `gpu/local-id` | `(fn (i64) i64)` | `` | false |
| `gpu/group-id` | `(fn (i64) i64)` | `` | false |
| `gpu/group-size` | `(fn (i64) i64)` | `` | false |
| `gpu/num-groups` | `(fn (i64) i64)` | `` | false |
| `gpu/global-id` | `(fn (i64) i64)` | `` | false |
| `gpu/barrier` | `(fn () unit)` | `` | false |
| `gpu/shared` | `(fn (keyword i64) ptr)` | `` | true |
| `gpu/host-index-set!` | `(fn (i64 i64 i64) unit)` | `` | false |
| `gpu/program-ptx` | `(fn () str)` | `` | false |
| `gpu/select` | `(fn (bool t t) t)` | `((Num t))` | false |
| `gpu/atomic-add-i32` | `(fn (ptr i32) i32)` | `` | true |
| `gpu/atomic-min-i32` | `(fn (ptr i32) i32)` | `` | true |
| `gpu/atomic-max-i32` | `(fn (ptr i32) i32)` | `` | true |
| `gpu/atomic-umin-i32` | `(fn (ptr i32) i32)` | `` | true |
| `gpu/atomic-umax-i32` | `(fn (ptr i32) i32)` | `` | true |
| `gpu/atomic-xchg-i32` | `(fn (ptr i32) i32)` | `` | true |
| `gpu/atomic-cas-i32` | `(fn (ptr i32 i32) i32)` | `` | true |
| `gpu/atomic-add-i64` | `(fn (ptr i64) i64)` | `` | true |
| `gpu/atomic-add-f32` | `(fn (ptr f32) f32)` | `` | true |
| `gpu/atomic-max-f32` | `(fn (ptr f32) f32)` | `` | true |
| `gpu/atomic-min-f32` | `(fn (ptr f32) f32)` | `` | true |
| `gpu/shfl-down-i32` | `(fn (i32 i32) i32)` | `` | false |
| `gpu/shfl-down-f32` | `(fn (f32 i32) f32)` | `` | false |
| `gpu/shfl-up-i32` | `(fn (i32 i32) i32)` | `` | false |
| `gpu/shfl-up-f32` | `(fn (f32 i32) f32)` | `` | false |
| `gpu/shfl-xor-i32` | `(fn (i32 i32) i32)` | `` | false |
| `gpu/shfl-xor-f32` | `(fn (f32 i32) f32)` | `` | false |
| `gpu/shfl-idx-i32` | `(fn (i32 i32) i32)` | `` | false |
| `gpu/shfl-idx-f32` | `(fn (f32 i32) f32)` | `` | false |
| `gpu/subgroup-size` | `(fn () i32)` | `` | false |
| `gpu/lane-id` | `(fn () i32)` | `` | false |
| `gpu/ballot` | `(fn (bool) i32)` | `` | false |
