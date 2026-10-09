# lair in fibber: the shared interfaces (package S1)

Current status: historical migration contract. Current signatures are in `compiler/lir/` and `compiler/native/`; the initial stub descriptions below describe wave 1, not today.

Original record (dated statements and unmarked code fences below are historical sketches):

Status: wave 1, package S1 (2026-10-04). This is the contract of the parallel packages R, K1, K2, L1, L2, C, J, A, H and F of
section 3.2 of `lair-in-fibber.md`. **The source of truth is the code**: `compiler/lir/` and `compiler/native/` hold every
signature, as a real definition (`lir.diag`, `lir.types`, `lir.lit`, `lir.ast`, and the records below) or as a stub whose
body is `(trap "todo: MODULE FUNCTION")` (spec/bootstrap.md 5.3), each with a comment naming the Rust file and function it
ports. This page says what the code cannot: who owns what, why the records are shaped as they are, and the answers to the open items of section 5.
`compiler/tests/native/skeleton.sh` builds a program that requires every module; it must keep printing `ok`.

## 1. Rules for every package

1. **Own your files, keep the signatures.** A package replaces the `trap` bodies of the files in its row (section 2) and adds private helpers
   (`:private`) or modules under its own directory. It does not edit another package's file. A signature, a record field, or a variant that
   must change is reported to the caller of the wave first; it is not changed quietly (the rule of spec/bootstrap.md 5.3).
2. **Names.** Every type is `L...` in `lir.*` (`LType`, `LExpr`, `LKind`, `LModule`..) and every variant carries a prefix of its enum (`TInt`,
   `KBin`, `BAdd`..; the full list is in `ast.fib`, `types.fib`). The reason: type and variant names are global across the modules a program links;
   the compiler already has `Module`, `Block`, `Item`, `Env`, `Header`, `Jit`, `Mailbox`, `Wait`, `Options`.
   (Checked: a copy of `compiler/fibc.fib` that also references one function of each of the 60 skeleton modules emits, so nothing collides.) In `native.*`
   the session of the JIT is `JitSession`, so that `native.api`'s `Jit` (the name of `lair.jit`) and the legacy `lair.jit` are not confused.
3. **Positions** are one `i64`, `line << 32 | col` (`lir.diag`: `pos-new`, `pos-line`, `pos-col`, `pos-str`, `pos-none` = 0, which is Rust's `Pos::default()`).
4. **Errors.** `lir.*` returns `(Result T Diagnostic)`; the whole-module checker returns `(Result unit (Vec Diagnostic))`. `native.*` returns
   `(Result T LairError)` (`native.error`: `ErrInvalid (Vec Diagnostic)`, `ErrInternal`, `ErrJit`, `ErrBackend`). Only `native.api` and `native.call`, which stand in
   for `lair.jit` and `lair.call`, return `(Result T str)`. A message is exactly the Rust's (the compare scripts judge it byte for byte).
5. **LLVM handles** (context, module, builder, type, value, block, target machine, LLJIT) are `i64`, as in `compiler/lair/ffi.fib`: an address, and 0 is
   null. A void result is `(Option i64)` nil. The bindings are `compiler/llvm/` (package 0A); nobody here writes an `extern` of LLVM-C.
6. **No global mutable state.** Context is a parameter. The mutable parts of a record are Cells inside it. The one exception to decide is in section 4 (the call shim).
7. **A payload-free variant allocates** (section 5, item 2). The parser and the lowering take the shared constants (`ty-ptr`, `ty-i32`, `kind-null`, `sc-system`..)
   and never write `TPtr` or `(TInt 32)` in a loop.
8. **Gotchas met while writing the skeleton** (each cost a compile): `defrecord` derives `Eq Ord Hash Show Debug`, so it needs all of them on every
   field; a record holding an enum uses `defstruct` plus `(derive Eq T)`. `(map f v)` is lazy (`LSeq`): wrap it in `vec`. `fn` and `def` are special forms
   and cannot be a field or parameter name. There are no unsigned types and no wrapping `+` (`unchecked-add` is not in the library yet): 128-bit
   arithmetic is done on 32-bit limbs (`lir.lit`). `shl`, `shr` (logical), `sar`, `bit-and/or/xor/not` do not trap. A module that nothing refers to is not loaded:
   the skeleton refers to one function of each.

## 2. Who owns which file

| Package | Files (all under `compiler/`) |
|---|---|
| **S1** (done, real) | `lir/diag.fib`, `lir/types.fib`, `lir/lit.fib` (added), `lir/ast.fib`; `tests/lir/unit-*.fib`; `tests/native/skeleton.{fib,sh}` |
| **R** reader | `lir/sexp.fib`, `lir/sexp/lexer.fib`, `lir/parse.fib`, `lir/parse/{names,ty,literal,expr,instr,memory,atomics,control,items}.fib`, `lir/dump.fib`, `lir/print.fib` |
| **K1** checker core | `lir.fib` (`parse-and-check`), `lir/check.fib`, `lir/check/{env,cfg,walk,fcx,globals,func}.fib` |
| **K2** checker rules | `lir/check/{phi,arith,aggr,calls,memory,expr}.fib` |
| **L1** lowering core | `native/{error,owned,passes,target,pipeline,lower}.fib`, `native/lower/{lx,types,consts,func}.fib` |
| **L2** lowering instructions | `native/lower/{expr,arith,calls,memory}.fib` |
| **C** call support | `native/{call,support}.fib` |
| **J** JIT | `native/{jit,api}.fib`, `native/jit/{names,prune,trampoline}.fib` |
| **A** AOT | `native/{aot,link}.fib` |
| **H** harness | `native/{cases,cli}.fib`, `native/cases/{header,exec,verdict}.fib`, and `lairf.fib` (not created by S1) |
| **O** oracles | not in `compiler/lir` or `native` (crates/lair/src/dump.rs, scripts, `tests/native/compare-*.sh`) |

Module graph (a module requires only what is above it; no cycles). The parts that were cycles in Rust are closed as in section 3.

```
lir:     diag types lit -> ast -> sexp.lexer -> sexp -> parse.names -> parse.{ty,literal} -> parse.{instr,memory,atomics,control} (take an ExprParser)
         -> parse.expr -> parse.items -> parse;  dump, print (ast only)
         check.{env,cfg,walk} -> check.fcx -> check.{phi,arith,aggr,calls} -> check.memory -> check.expr -> check.{globals,func} -> check -> lir
native:   error -> {owned,passes,target} -> lower.lx -> lower.{types,consts} -> lower.{expr,arith,calls,memory} -> lower.func -> lower -> pipeline
          jit.{names,prune,trampoline} -> jit -> api;  support, call;  link -> aot;  cases.{header,exec} -> cases.verdict -> cases;  cli
```

`lir.check.expr` (the dispatcher) requires `phi arith aggr calls memory`; `phi` and `globals` use `constant?`, which therefore lives in `lir.check.walk`
(with `emits?`), not in `expr` as in Rust. `lir.check.memory` uses `check-step` of `aggr`. `native.lower.lx` is a leaf that is not in the module map of
section 1.2: Rust's `lower/mod.rs` is the parent of the other lowering files and they use its records, and fibber has no parent module.
Likewise `lir.parse.names` carries `arity`, `atom-text` and `label`, which Rust keeps in `parse/mod.rs`.

## 3. The shared records and the closed cycles

### 3.1 The checker: `Env`, `Cfg`, `Fcx` (K1 owns the records, K2 uses them)

```
(defenum LSymbol (SymFunc ty: LFnType) (SymVar ty: LType constant: bool))
(defstruct LEnv (structs: (Map str (Vec LType)) symbols: (Map str LSymbol)))                          ;; lir.check.env
(defstruct Cfg (succs: (Vec (Vec i64)) preds: (Vec (Vec i64)) rpo: (Vec i64) idom: (Vec i64)))      ;; lir.check.cfg; idom -1 = None
(defstruct Bindings (defs: (Map str i64) allocas: (Map str LType)))                                    ;; lir.check.walk; defs -1 = a parameter
(defstruct PendingPhi (block: i64 ty: LType incoming: (Vec LBinding) pos: i64))                       ;; lir.check.fcx
(defstruct Fcx
  (env: LEnv f: LFunction cfg: Cfg labels: (Map str i64) defs: (Map str i64) allocas: (Map str LType)
   types: (Cell (Map str LType)) cur: (Cell i64) emitted: (Cell bool) phis: (Cell (Vec PendingPhi))
   ty: (fn (Fcx LExpr) (Result (Option LType) Diagnostic))          ;; THE DISPATCHER FIELD
   term: (fn (Fcx LExpr) (Result unit Diagnostic))))                ;; the terminator checker, the second dispatcher
```

**The cycle.** In Rust `Fcx` has methods in seven files that call one another: `ty` (expr.rs) dispatches to `bin` (arith.rs), which calls `val` (fcx.rs), which calls
`ty`; `stmt` (fcx.rs) calls `terminator` (calls.rs), which calls `val`. Modules cannot cycle, so `lir.check.fcx` holds the context and everything that needs no rule
(`fcx-val`, `fcx-stmt`, `fcx-effect`, `fcx-let-form`, `fcx-lookup`, `fcx-lookup-at-end`, `fcx-valid`, `fcx-same`, `fcx-block`), and **`ty` and `term` are fields of the
context**: `fcx-val` calls `((. fcx ty) fcx e)`. `lir.check.func/check-function` builds the context with `check-ty` (`lir.check.expr`) and `check-terminator`
(`lir.check.calls`) and so closes the cycle once per function. The rule modules take the context and call `fcx-val` or `fcx-ty` for their operands, **in the
order the Rust calls them**, so the order of diagnostics is the Rust's. `fcx-val` is `val` of fcx.rs (it rejects a terminator and a void value, with the Rust's messages).

**Which rules need the dispatcher field** (open item of section 5). Every rule that types an operand *between* its own checks, which is nearly all of them, so the
answer is "all of K2's rules go through `fcx-val`", and what is pure (no operand typing) is a plain function of types:

| Needs the field (calls `fcx-val`/`fcx-ty`) | Pure on types (no field) |
|---|---|
| `check-bin` (type a, check kind, type b, check same, divide by zero, shift width), `check-overflow`, `check-un`, `check-cmp`, `check-select`, `check-cast` (valid, then type), `check-struct-literal`, `check-array-literal`, `check-vector-op`, `check-extract-value`, `check-insert-value` (type a, path, type v), `check-call` (callee, then arguments one by one, each compared at once), `check-terminator` (`ret`, `switch`, `condbr`, the tail rules), `check-memory` (every form: `store` types the value, then the pointer; `gep` types indices in a loop), `check-phi`/`finish-phis` (`lookup_at_end`, `val` under a saved cursor) | `cast-rule`, `bitcast_rule`, `check-step` (given a type and an index), `result-in-registers`, `leaves`, `literal_lanes`, `mask`, `atomic_type`, `forbid`, `direct` (reads `allocas` and `env` only) |

`select` types all three operands before any check, so it could be pure; it takes the context anyway, for uniformity. K2 may not rely on the pure column being large: the
compare script `compare-check.sh` judges the order of errors and that is what decides.

### 3.2 The lowering: `Lx`, `Fx` (L1 owns the records, L2 uses them)

```
(defstruct FuncEntry (value: i64 ty: LFnType))
(defstruct Lx (ctx: i64 module: i64 td: i64 structs: (Cell (Map str i64)) struct-fields: (Cell (Map str (Vec LType)))
               funcs: (Cell (Map str FuncEntry)) globals: (Cell (Map str i64))))
(defstruct PhiFix (phi: i64 incoming: (Vec LBinding)))
(defstruct Fx (lx: Lx b: i64 blocks: (Vec i64) labels: (Map str i64) names: (Cell (Map str i64)) phis: (Cell (Vec PhiFix))
               expr: (fn (Fx LExpr) (Result (Option i64) LairError))))                                 ;; the dispatcher field
```

All in `native.lower.lx` with `fx-expr`, `fx-val`, `cc-number`, `tailcc-number`. `native.lower.func/lx-function` builds the `Fx` with `native.lower.expr/lower-expr` as `expr`.
The instruction modules keep the Rust shape (`(lower-arith fx e)`, `(lower-call fx e)`, `(lower-control fx e)`, `(lower-memory fx e)`, `(lower-trap fx)`) and lower their operands with
`fx-val`, left to right, then make the instruction, so the LLVM text has the Rust's order of instructions. (The design doc's "lower-bin taking LLVM values" is the same
thing split further; the compare script `compare-llvm.sh` decides, and L2 may split inside its own files.) Lowering declares every item first and then lowers bodies
(`lower/mod.rs` `module_items`): `native.lower/lower` does that.

### 3.3 The reader: `Sexp`, and how the grammar recursion is closed

`lir.sexp` has the `Sexp` enum (`SxList SxBrace SxBracket SxAtom SxStr SxVecType`, each with its position; `SxStr` holds `(Array i8)`) and `read : str -> (Result (Vec Sexp) Diagnostic)`.
`lir.sexp.lexer` is R's own (its `Tok`/`tokens` are the Rust shape and may become streaming: only `read` is fixed). `&[Sexp]` of Rust is `(Vec Sexp)`: the operands after the head,
copied once per list. In the parser `parse-expr` calls `parse-instr`, `parse-memory`, `parse-atomic` and `parse-control`, which call `parse-expr` for their operands, so the four take an
`ExprParser` (`lir.parse.names`: `(defstruct ExprParser (parse: (fn (Sexp) (Result LExpr Diagnostic))))`) as their last parameter, and answer
`(Option (Result LExpr Diagnostic))`, nil meaning "not my head". `lir.parse.expr/expr-parser` makes the value.

`lir.parse/parse-source : str -> (Result LModule Diagnostic)` is `lir::parse` (reads all forms first, so a lexical error anywhere is reported before a parse error).
`lir/parse-and-check : str -> (Result LModule (Vec Diagnostic))` is `lir::parse_and_check` (K1 writes it; it requires `lir.parse` and `lir.check`).
`lir.dump/dump-module`, `dump-diagnostic` and `lir.print/print-module`, `print-expr` are R's, for the dump grammar of package O (`docs/design/lair-ast-dump.md`).

### 3.4 The data of `lir.ast` (real)

`LModule (items: (Vec LItem))`; `LItem` = `ItStruct LStructDef | ItGlobal LGlobalDef | ItDeclareGlobal LGlobalDecl | ItDeclare LDeclare | ItDefine LFunction`; `LFunction (name ty: LFnType params: (Vec LParam)
blocks: (Vec LBlock) mods: LModifiers pos)`; `LBlock (label body: (Vec LExpr) pos)`; `LExpr (kind: LKind pos: i64)`; `LKind` has the 40 variants of `ast.rs::Kind` in the Rust order with the
Rust's field names (`KBin op a b`, `KCall callee args tail`, `KSwitch v default cases: (Vec LCase)`..). The operator enums are `LBinOp LUnOp LOvfOp LCastOp LIPred LFPred LOrdering LRmwOp LScope LLinkage`.
`Box<Expr>` is a plain field. Rust's `(String, Pos)` of a parameter is `LParam`, the `(Expr, String)` of a `switch` case is `LCase`, a `u32` alignment is an `i64`.
Helpers: `expr-new`, `kind-terminator?`, `expr-int-literal`, `linkage-exported?`, `function-variadic?`, `ordering-rank` (for the derived `Ord`).

## 4. Decisions S1 took and the call shim

* **Integer literals are 128-bit** (`lir.lit`: `(defrecord LLit (hi: i64 lo: i64))`), not the (low 64 bits, sign) of the design doc. The reason: `(iK n)` admits
  `-2^(K-1) <= n <= 2^K - 1` for `K <= 64` (`parse/literal.rs:37`, 65 bits plus sign, which the pair would carry), but the **index lists of `extractvalue` and `insertvalue` are `Vec<i128>`**
  (`parse/instr.rs:176`, `int_token` with no range check) and are printed in diagnostics (`extractvalue: field index N out of range for T`, `check/aggr.rs:69-79`); only a full
  i128 reproduces those messages byte for byte. `int_token` accepts any magnitude up to `i128::MAX` and refuses more: `lit-mul-add` returns nil where `from_str_radix` fails. Functions:
  `lit-of`, `lit-add`, `lit-neg`, `lit-cmp`, `lit-lt?`, `lit-pow2`, `lit-low` (= `(v as u128) & mask(bits)`, bits 1 to 64, as a word compared with `u64-lt`), `lit-fits-int?` (= `int_fits`), `lit->i64`,
  `lit-mul-add`, `lit-str` (the signed decimal Rust prints). Tested in `compiler/tests/lir/unit-lit.fib` against the values Rust's `i128` gives, including the 41 nines of the Rust test.
* **`Kind::Str` is `(Array i8)`** (open item 1, section 5).
* **Array length** `TArray n` is an `i64`: the parser refuses `N > u32::MAX` (`parse/ty.rs:62`). Vector lanes are 1 to 1024. Alignment is an `i64`.
* **`native.api` and `native.call`** have exactly the names, parameters and results of `compiler/lair/jit.fib` and `call.fib`, so the cutover is a changed `:require`. `native.api` has the `jit.fib`
  names (`jit-new`, `jit-new-fast-codegen`, `jit-free`, `jit-add-source`, `jit-address`, `jit-c-entry`, `check-source`, `build-executable`), `native.call` has the `call.fib` names (`call-i64`, `call-f64`, `hook-addresses`, `call-new`,
  `call-free`, `call-fault`, `call-start`, `call-wait`, `call-hook-arg`, `call-hook-reply`, `call-result`, and `Hooks`, `Mailbox`, `Wait`, `max-args`). The users also call `lair.ffi` (`bytes-new`, `text-at`, ..): that
  module stays, minus its `lair_error_*` externs, at the cutover.
* **The call shim has no handle in its signature, and C must decide where it lives.** `call-i64 addr args` takes no session, but the shim (design 1.5, option (a)) is lIR that some JIT session compiled, and
  fibber has no global. Options for C, in order: (1) the shim is compiled once into a session the module keeps in a top-level `def` of a Cell, initialised on first call, the one deliberate piece of process-wide
  state (like LLVM's own target registry), documented as such; (2) the shim's address is passed between calls through the environment (`FIB_LAIR_SHIM`); (3) the signature gains the session and every user changes.
  S1 fixed the signatures as `lair.call`'s; if C chooses (3), tell the caller before any user is edited.
* **`native.jit.names` is persistent**: `names-record` returns the new `JitNames`; the session holds it in a Cell.

## 5. The open items of section 5 of the design

**1. May a `str` hold non-UTF-8 bytes? No, so `Kind::Str` is `(Array i8)`.** Measured with the seed (v0.1.4):

```
(let [a (array 2 -1i8) s (str-from-bytes a)] (str-len s))
trap: str-from-bytes: invalid UTF-8
```

(spec/syntax.md: "a string is UTF-8"; `str-from-bytes` traps on anything else.) lIR strings do contain such bytes: `\xHH` is "one byte" (spec/lir.md section 1; `sexp/lexer.rs:133`) and the case
`cases/lir/instr/string.lir:11` is `(string "\xff")`. The emitter itself never writes a byte above 0x7f in a string (it writes `(i8 195)` in arrays: checked on an `é` + emoji literal, zero `\x` in the output), but
`lairf` must accept the Rust's inputs. The lexer works on `str-byte-at`, builds the bytes of a literal as an `(Array i8)` (`array-push!`), and `lower/consts.rs::string` hands the array to LLVM.
A source file that is not UTF-8 is refused before the lexer, as `read-file` returns nil for it (the Rust `read_to_string` refuses it too).

**2. Is a payload-free variant a shared constant? No: it allocates.** Measured with `FIB_TRACE=1 fibc run` counting `A` lines (the programs are in the scratch directory
`~/.cache/fibber-scratch/LAIR1-S1/p/`): a loop of N calls of `(defun mk (i) (if (> i 0) Fl Db))` with the result matched, `Fl` and `Db` the payload-free variants of an enum.

```
b.fib   (N = 1000)  4375573        d.fib  (a bool instead of the enum, N = 3000)  4360924
b2.fib  (N = 3000)  4377573        d1.fib (a bool, N = 1000)                       4360924
```

N grew by 2000 and the enum version made 2000 more allocations, one per variant written; the bool version made none. A top-level constant does not:

```
f.fib   (N = 3000, mk returns (def t-fl: T Fl))  4379133      f1.fib (N = 1000)  4379133
```

So `lir.types` and `lir.ast` define `ty-i1 ty-i8 ty-i16 ty-i32 ty-i64 ty-float ty-double ty-ptr`, `cc-c cc-tail`, `sc-system sc-single-thread`, `lk-external lk-internal lk-private`, `kind-null kind-trap kind-unreachable`
as `def`s (made once, then only counted), and the parser must use them for the forms it meets most; the operator enums (`BAdd`, `IEq`..) allocate one small object per use, which the
design accepted (one per instruction node); R measures `lairf check` on the 19.1 MB file and takes the constant-table route for the operators if it matters (R4).

**3. Which check rules need the dispatcher field?** Section 3.1: every rule that types an operand through `fcx-val`, which is all of K2's, plus the statement-level `terminator`, which is a second field. `lir.check.expr/check-ty` is the value of the
first; `lir.check.calls/check-terminator` of the second.

## 6. A disagreement found, not changed

`spec/compiler.md` section 9 says the C interface has 22 functions and ROADMAP.md says 21; `crates/lair/include/lair.h` declares 23 `lair_*` functions (design doc section 0: `nm -D --defined-only liblair.so | grep -c ' T lair_'` prints 23). Reported in the S1
report, neither document changed.
