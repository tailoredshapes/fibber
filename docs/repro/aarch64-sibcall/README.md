# aarch64-sibcall

The reproducers of docs/design/aarch64-sibcall.md (verdict: an LLVM AArch64 backend bug). No script: the commands.

| File | What |
|------|------|
| `min.ll` | tailcc function, ten i64 arguments, unmarked call of a ccc function |
| `min-tail.ll` | the same with `tail call` |
| `min-frame.ll` | with a local frame, to see the `sub sp` / `add sp` pair |
| `fibber-prefix.ll` | lair's output for `cases/lir/instr/stackargs-sibcall.lir` as 0.1.12 emitted it (no `notail`), aarch64-linux |
| `swifttail.c` | the same shape in C, clang `swiftasynccall` (LLVM swifttailcc) |

```
make repro-aarch64-sibcall LLC=/usr/lib/llvm-21/bin/llc          # from the repository root; bl is right, b is the fault

llc -mtriple=aarch64-linux-gnu -O2 min.ll -o -                   # bl callee  (correct)
llc -mtriple=aarch64-linux-gnu -O2 min-tail.ll -o -              # b callee   (no pop of the 16 bytes of arguments)
opt -O2 -S min.ll | llc -mtriple=aarch64-linux-gnu -O2 -o -      # b callee   (opt adds the tail marker itself)
llc -mtriple=aarch64-linux-gnu -O2 min-frame.ll -o -             # sub sp,sp,#32 ... str x8,[sp],#32 ; b callee

# C, no hardware: the assembly
clang --target=aarch64-linux-gnu -O2 -S swifttail.c -o -         # pop ends in `b callee`
# C, run on an Apple Silicon Mac (-O0 prints ok; -O1 and up crash with SIGSEGV)
clang -O2 swifttail.c -o swifttail && ./swifttail
# C, run under qemu on x86 Linux
clang --target=aarch64-linux-gnu -O2 -c swifttail.c -o sw.o && aarch64-linux-gnu-gcc sw.o -o sw && qemu-aarch64 -L /usr/aarch64-linux-gnu ./sw
```
