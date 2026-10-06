# cases/modules

Programs of several modules (syntax §5). Each case is a directory:
its `main.fib` carries the header (method.md rule 3) and the `ns`
form whose `:require` and `:use` clauses name the other modules,
which live beside it, `a.b` at `a/b.fib`. Both harnesses run a
directory's `main.fib` as one case (`fibref cases cases/modules`,
`fibc cases cases/modules`), and every case runs interpreted and
compiled with matching results and free traces (method.md rule 6).

**Library roots** (syntax §5, compiler.md §1; **Proposed**). A module is
found beside the main file, then under each root in order. A case that
needs a root says so in its header, with the key `roots`: directories
separated by white space, relative to the case's directory, in the order
they are searched (`;; roots: first second`). The interpreter side of
the harness gets them as the roots of `fibref::roots::Roots`, and the
child `fibc run` as one `-I DIR` each, so `fibref run -I DIR main.fib` and
`fibc run -I DIR main.fib` run the case by hand. A case's roots are its
header's alone: `FIB_LIB` is not read by either side (the harness removes
it from the child's environment), so a case gives the same answer on
every machine. A subdirectory of a case that holds roots has no
`main.fib`, so it is not mistaken for a case.

- 001: a `:require` under an alias and a `:use`, three modules read
  once each in dependency order (17).
- 002: a `:private` definition of a required module is not reachable by
  its qualified name (reject: `secret is private to util; it is not
  exported`).
- 003: `(var u/secret)` is the one reference that reaches it (syntax
  §3.20), beside a public function of the same module (83).
- 004: two modules requiring a third: it is one module, its struct one
  type, its `def` one value (7).
- 005: macros through a `:use` unqualified and through an alias, two
  modules' macros of one name told apart (40).
- 006: a `:private` macro is not reached through an alias (reject:
  `unbound name u/hidden`).
- 007: the program's own protocol `Collection` with a method `conj`,
  implemented for `(Vec a)`, beside the library's (`fib.coll`, implicit
  since the flip): the program's method shadows the library's for a bare
  `conj` (syntax §5), the vector literal is the prelude's `vec-conj` and
  `fib.coll/conj` the library's (43). `fibc` named both
  `m.Collection.conj.$Vec..` and said `duplicate definition`; the symbol of
  a protocol's method and of its vtable now carries the defining module.
- 008: two modules each define a protocol `Sized` with a method `size`
  and implement it for `str`: two protocols, called directly and through
  `(dyn Sized)` (2121). `fibc` named both `m.Sized.size.str`, and a
  vtable of the second would have been the first's.
- 009: two `:use`d modules export `peek`, and a bare `(peek 3)` is an
  error naming both modules (reject: `peek is exported by both one and two;
  write one/peek or two/peek`). Syntax §5 says so; the code took the first
  `:use` silently (3 with `(:use one two)`, 4 with `(:use two one)`). The
  prelude is not a second `:use`: a `:use`d module's name shadows the
  prelude's, as it always did.
- 010: what 009 does not reject: a name this module defines itself
  shadows both, a name only one exports is no clash, and `one/peek`,
  `two/peek` name each module's (1630).
- 011: the same rule for a macro two `:use`d modules both define
  (reject: `twice is exported by both one and two`).
- 012: a quasiquote template is rewritten into calls that resolve in the
  prelude, so a library's own `concat` and an enum with variants named
  `List`, `Vec` and `Map` do not capture them, and `` `() `` expands under
  `fibc` as it does in the interpreter (62).
- 013: a library module found under a root, and the module it needs
  found there too (43).
- 014: the order of the search: the main file's directory, then each root
  in order; a module in all three places is the main directory's, in two
  roots the first's, and the second's file is never read (108).
- 015: a facade that re-exports with `(:export-from a b)`: functions,
  a struct and its constructor, a protocol and its method and a macro,
  bare through a `:use` and through the alias of a `:require` (716).
- 016: a module in no root (reject: `module ext.nowhere is not at`).
- 025: two modules each define types of one name with different layouts
  (a struct `Pt`, a private struct `Hidden`, a generic struct `(Wrap Pt)`,
  an enum `Shape`, each with an impl of a protocol `Sz` of one name), and
  the main module a `Box` beside the prelude's: an object struct and a
  specialisation carry the defining module's name, as a protocol's
  symbols do (8191). `fibc` named them by the bare type name: `getelementptr:
  field index 4 out of range for %struct.o.Pt` and `duplicate definition
  of @m.Sz.size.Pt`.
- 029: a module defines its own list type `(Stack a)` with variants spelt
  `Empty` and `Cons`, and the main module uses it: `(list ..)` still
  builds the prelude's `List` (`fib.prelude/Cons`, `fib.prelude/Empty`).
  Case 020's lowercase `cons` and `empty` cannot capture the expansion
  any more; this one can (223).
- 030: a module defines functions named `vec-empty`, `vec-conj`,
  `map-empty` and `map-assoc` that build the wrong collections, and the
  main module uses it: `[..]` and `{..}` are still the prelude's (23).
  Case 021's `conj` and `assoc` are no longer what a literal is built with.
- 031 to 035 (stage 2, syntax §3.16 "Names in a template" and
  "Macro-time helpers", docs/design/macro-names.md): a library's macros
  through a `:require` alias alone, their templates naming the library's
  functions bare, a function of a module the library requires through
  its alias and a macro of its own (which recurses by name), a local the
  template binds kept local (031, 1279); a template that names a
  `:private` function and `def` of its module (032, 744); locals at the
  site spelt like the template's names do not capture them (033, 1010;
  resolved at the site it is 5007); the site's own functions spelt like
  them are not called (034, 24; resolved at the site it is 598); a
  macro calls, at expansion time, recursive helpers of a module its
  module requires (035, 59).
