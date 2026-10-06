# 0008. Supported ISAs have guaranteed tail calls and FMA

Status: accepted
Date: 2026-10-06
Source: the owner, 2026-10-06 (docs/design/decisions-2026-10-04.md, "Supported instruction sets"); compiler/types/targets.fib; docs/design/targets.md.

## Context

fibber's loops are tail calls: `recur` and calls in tail position are lIR `tailcall`s, and spec/lir.md 7.3 promises that they do not grow
the stack. LLVM keeps that promise only under its `tailcc` convention with `musttail`, which it implements for x86-64 and AArch64 but not
for RISC-V (LLVM 21: "Unsupported calling convention") nor WebAssembly (the tail-call extension's `return_call`, C convention only). The
numeric library leans on fused multiply-add: `simd/fma` is one exact rounding, and without hardware it was a libm call per lane, about 30
times slower (ADR 0004). Until now the release was built for x86-64-v2, which has no FMA, so every fused kernel needed a second, unfused
copy. The owner: "I'm comfortable saying we only support isa that have tail call capabilities. I am also happy to say the same thing about
FMA. Its 2026. This language is partly about making modern capabilities like simd and multiple cores easier to access." And: "For systems
where FMA is simply unavailable we emit a warning / trap if its used on that system."

## Decision

A native target is **supported** only when its toolchain guarantees a tail call between functions of any signature (LLVM `tailcc` with
`musttail`) **and** its instruction set has fused multiply-add. The table of targets (`compiler/types/targets.fib`, `fibc targets`) says so
per row in its `support` column, and `row-supported?` checks the facts as well as the mark.

- x86-64 is supported at **x86-64-v3** (AVX2, FMA, BMI1/2, F16C, LZCNT, MOVBE): the row's CPU for cross builds and releases
  (`scripts/package.sh`); a build for the host keeps the host's CPU. Every x86-64 Linux executable checks at start that the CPU has
  what it was built for and otherwise traps with `this program needs x86-64-v3 (AVX2, FMA); this CPU lacks: ..` (`emit.cpucheck`).
- AArch64 (Darwin, Linux, iOS) is supported unchanged: FMA is in its baseline, so it needs no start-up check.
- riscv64 is **parked**: LLVM has no `tailcc` for RISC-V. `fibc build --target` refuses it unless `--allow-unsupported` is given;
  its emit check (`compiler/tests/native/targets-emit.sh`) stays, to notice when `tailcc` lands.
- wasm32 (WASI and the browser) is a **portability target**: built, outside the rule.
- On any target without FMA (wasm, or an x86-64 CPU named below x86-64-v3), `simd/fma` is a compile-time warning at each call
  site (`FILE:LINE:COL: warning: simd/fma has no hardware support on T; it traps at run time: use simd/muladd for portable code`) and a
  trap with that text at run time: never a libm call, never an emulation. `simd/muladd` stays legal everywhere (fused where available, a
  multiply and an add otherwise).

## Consequences

- Releases need a 2013+ x86-64 CPU with AVX2 and FMA (or Apple Silicon/ARMv8) and glibc 2.33 or later. An older x86-64 CPU gets a one-line
  message at start, not SIGILL later.
- The unfused fallbacks of the library (the `(has-fma)` false branches, the mul+add GEMM tiles) are dead on every supported target and
  can be removed; until they are, a `(has-fma)` dispatch on a target without FMA lowers only its plain branch (an `if` on the literal
  `false` lowers its else branch alone), so it neither warns nor runs a fused kernel.
- ADR 0004's "a safe one on x86-64 and x86-64-v2 (what a release ships)" no longer describes the release; its rules still hold.
- A target that later gains guaranteed tail calls (RISC-V with `tailcc`) or FMA becomes supported by changing its row, and this ADR's rules
  check the change.
- Not checked: an AArch64 build for a CPU with features beyond the baseline (`FIB_TARGET_CPU=apple-m2`) has no start-up check; a
  non-Linux x86-64 executable has none either (there is no such row).

## Governance

The rules read the table's `TargetRow` literals in `compiler/types/targets.fib` (24 fields after the head: 12 `has-fma`, 14 `tail-cc`,
15 `tail-kind`, 24 `support`), so a planted row is seen exactly as a real one would be.

```fibber fitness
(defun rows-in (n: Node) -> (Vec Node)
  (match n
    ((Atom _ _) [])
    ((Seq _ items _ _)
     (let ((below (reduce (fn (acc: (Vec Node) x: Node) (into acc (rows-in x))) [] items)))
       (if (and (= (count items) 25) (= (atom-text (nth items 0)) "TargetRow")) (into [n] below) below)))))

(defun table-rows (repo: Repo) -> (Vec Node)
  (reduce (fn (acc: (Vec Node) f: SrcFile) (reduce (fn (a: (Vec Node) t: Top) (into a (rows-in (. t node)))) acc (. f tops)))
          [] (select repo ["compiler/types/targets.fib"])))

(defun field (row: Node i: i64) -> str (atom-text (nth (items-of row) i)))

(defun row-finding (row: Node why: str) -> Finding (Finding "compiler/types/targets.fib" (node-line row) (str (field row 1) ": " why)))

(defun supported-row? (row: Node) -> bool (= (field row 24) "\"supported\""))

(rule "every row marked supported has LLVM's tailcc (18), musttail (tail-kind 2) and fma"
  (mapv (fn (r: Node) (row-finding r "marked supported without tailcc 18, musttail and fma"))
        (filterv (fn (r: Node) (and (supported-row? r) (not (and (= (field r 12) "true") (and (= (field r 14) "18") (= (field r 15) "2"))))))
                 (table-rows repo)))
  (plant "compiler/types/targets.fib" "\n(defun row-bad () -> TargetRow (TargetRow \"wasm64-x\" \"wasm64\" \"wasi\" \"Linux\" \"\" 64 true \"none\" 128 \"generic\" \"\" false 0 0 1 1 \"none\" \"wasm\" \"wasm-ld\" \"\" \"none\" [] \"emits\" \"supported\"))\n"))

(rule "riscv64 has a row, and it is parked, not supported"
  (let ((rv (filterv (fn (r: Node) (= (field r 2) "\"riscv64\"")) (table-rows repo))))
    (if (empty? rv)
        [(Finding "compiler/types/targets.fib" 1 "no riscv64 row")]
        (mapv (fn (r: Node) (row-finding r "riscv64 is not parked")) (filterv (fn (r: Node) (not (starts-with? (field r 24) "\"parked"))) rv))))
  (plant "compiler/types/targets.fib" "\n(defun row-rv-bad () -> TargetRow (TargetRow \"riscv64-x\" \"riscv64\" \"linux\" \"Linux\" \"\" 64 true \"pthreads\" 128 \"generic-rv64\" \"\" true 0 18 2 2 \"dwarf-eh\" \"elf\" \"cc\" \"\" \"glibc\" [] \"emits\" \"supported\"))\n"))

(rule "the x86_64 and aarch64 rows are supported, and x86_64's CPU is x86-64-v3"
  (into (mapv (fn (r: Node) (row-finding r "not marked supported"))
              (filterv (fn (r: Node) (and (or (= (field r 2) "\"x86_64\"") (= (field r 2) "\"aarch64\"")) (not (supported-row? r)))) (table-rows repo)))
        (mapv (fn (r: Node) (row-finding r "x86_64's CPU is not x86-64-v3"))
              (filterv (fn (r: Node) (and (= (field r 2) "\"x86_64\"") (not (= (field r 10) "\"x86-64-v3\"")))) (table-rows repo))))
  (plant "compiler/types/targets.fib" "\n(defun row-x-bad () -> TargetRow (TargetRow \"x86_64-x\" \"x86_64\" \"linux\" \"Linux\" \"\" 64 true \"pthreads\" 128 \"x86-64-v2\" \"\" true 0 18 2 2 \"dwarf-eh\" \"elf\" \"cc\" \"\" \"glibc\" [] \"runs\" \"supported\"))\n"))

(rule "the rules above read the table: at least seven rows, four of them supported (an unreadable table would pass them all)"
  (let ((rs (table-rows repo)))
    (if (and (>= (count rs) 7) (>= (count (filterv supported-row? rs)) 4)) []
        [(Finding "compiler/types/targets.fib" 1 (str (count rs) " TargetRow literals, " (count (filterv supported-row? rs)) " supported"))]))
  (plant-file "compiler/types/targets.fib" "(ns types.targets)\n"))

(rule "the release baseline in scripts/package.sh is x86-64-v3, and no live line of it names x86-64-v2"
  (into (must-contain repo "scripts/package.sh" "plat=linux-x86_64;  default_cpu=x86-64-v3;") (grep-live repo ["scripts/package.sh"] "x86-64-v2"))
  (plant "scripts/package.sh" "\nexport FIB_TARGET_CPU=x86-64-v2\n"))

(rule "simd/fma without FMA lowers to the trap (emit.lower.simdfn), and executables call the start-up CPU check (emit.compile)"
  (into (must-contain repo "compiler/emit/lower/simdfn.fib" "(fma-trap cx)") (must-contain repo "compiler/emit/compile.fib" "(cc/cpu-check-call t)"))
  (plant-file "compiler/emit/lower/simdfn.fib" "(ns emit.lower.simdfn)\n"))
```

### What this does not check

That the rows' facts are true of LLVM (`compiler/tests/native/targets-emit.sh` and `a64-emit.sh` look at real objects and assembly); that
the warning and the trap behave (`compiler/tests/driver/no-fma.sh`, its planted faults in `no-fma-faults.sh`); that the start-up check
traps on a CPU that lacks a feature (`compiler/tests/driver/cpu-check.sh`: a planted AVX-512 requirement, and a v3 binary under
`qemu-x86_64 -cpu Westmere`); that the release really runs on an x86-64-v3 CPU (package.sh runs it on the machine that builds it); a
row built some other way than a `TargetRow` literal of 25 items (the fourth rule notices when the table stops being read).
