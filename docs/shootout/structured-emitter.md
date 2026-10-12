# Structured emitter handoff

Measured on 2026-10-12 on the Linux development host. This checks the handoff
introduced by [ADR 0025](../adr/0025-the-emitter-builds-structured-lir.md).

The baseline compiler is main at `cf467414`; the structured compiler is this
change. Both are whole-program optimized executables linked to shared LLVM 21.
Both read the same current compiler sources and unchanged prelude. Compiled
object reuse is disabled. These are one sequential run per compiler, after the
local gates had warmed filesystem caches; they are samples, not medians.

```sh
FIB_LIRO_CACHE=off FIB_TIME=1 /usr/bin/time -v "$COMPILER" \
  build compiler/fibc.fib -Icompiler -Ilib --emit llvm -O 0 -o "$OUTPUT"
```

| Measurement | Baseline | Structured |
|---|---:|---:|
| Front end | 12.404 s | 12.800 s |
| LIR emission | 12.588 s | 11.746 s |
| LLVM lowering | 1.131 s | 1.128 s |
| Total wall time | 29.36 s | 27.41 s |
| Maximum RSS | 1,462,900 KiB | 1,464,036 KiB |

The wall-time sample improved by 1.95 seconds, about 6.6%; peak memory was
essentially unchanged. The emission timer has a different boundary: the
baseline includes LIR serialization, while the structured path hands nodes
to verification directly. Parsing/checking and LLVM output writing are included
in total wall time but are not separate timers for `--emit llvm`.

Both runs exited zero. After removing only LLVM's `ModuleID` and
`source_filename` header lines, their LLVM output was byte-for-byte identical:
41,220,385 bytes, SHA-256
`279aa2cf1de759def23e3bbc080c5ef557bc23e7963df088e502d377bec028ad`.

This experiment deliberately excludes LLVM optimization, machine-code
selection, and linking. It does not establish an equivalent reduction in
complete optimized executable builds or hosted CI time. Copying and dense
compiler tables remain separate work.
