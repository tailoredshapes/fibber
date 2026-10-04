# stack-cell-drop: stage 2 drops a stack cell inline (a note, not work)

The Rust is frozen (seed-1); this records a deliberate divergence in the emitted lIR so nobody reads it as a bug.

Performance batch 3, lever P1: `compiler/emit/lower/ops.fib` `end-stack` takes the type of the site it ends
(`site-ty`). For a stack object of type `(Cell T)` it no longer emits `(call @fib.drop-fields w)` (a 256-byte worklist
on the stack, an indirect call of `drop.N`, a drain) but the release of the one counted slot in line: a load of the
slot and `(call @fib.release x)` when `T` lowers to a pointer or a `dyn`, and nothing when `T` is a scalar or unit.
`fib.stack-end` is emitted first, as before; the order of the releases, the frees and the trace lines is the same.
Every other stack object still drops through `fib.drop-fields`.

Effect: `fibc emit` of the Rust and of stage 2 differ at each `end-stack` of a cell (the library's cell idiom, `(cell x)`
with `array-set!`, `array-take!`, `array-push!`, `&added`). The cases agree: `cases/ownership` (all), `cases/stdlib` 40*.
Not ported, and not to be: the Rust is the seed.
