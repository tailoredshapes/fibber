# Method

liar was abandoned because its documentation, commit messages and test
suite claimed more than the code did. fibber is built so that no claim
rests on anyone's word, including the author's.

## Rules

1. **The spec is executable.** Every rule in the prose spec is also
   implemented in a small reference interpreter that follows the rules
   literally and optimises nothing. Where the prose and the interpreter
   disagree, that is a bug in one of them, found and fixed before
   anything else.

2. **The interpreter audits memory.** Every heap allocation, retain,
   release, free and access goes through an instrumented heap. A run
   fails on use-after-free, double free, a count going negative, or any
   allocation still live at exit that is not on a documented leak path
   (see [ownership.md](ownership.md), cycles).

3. **Cases come before rules.** Each case in `cases/` states its
   expected verdict in its header before the rules that decide it are
   final:
   - `accept`: must type-check, run, produce the stated result, and
     finish with a clean memory audit. A header may add `allocs: <= N`:
     the run allocates at most `N` heap objects, the `A` lines of its
     free trace (compiler.md §4). `fibref cases` counts them in the
     interpreter's trace and `fibc cases` in the `FIB_TRACE=1` trace of
     the compiled run; the two traces are the same lines, so the counts
     are the same, except in a threaded run, where the compiled program
     may allocate more. More than `N` is a failure, never pending, and
     so is an evaluator that gives no count: a bound nobody checked
     would pass whatever the program did. A `<=` fails only when the
     count goes up, so a bound is worth having when `N` is the count,
     and a case that claims that has a test that lowers `N` by one and
     requires the failure.
   - `reject`: must fail to compile, with an error containing the
     stated text.
   - `trap`: must type-check and pass the ownership checker, then
     fail at run time with a trap whose message contains the stated
     text. A trap aborts the program (types §2.11), so the objects
     live then are not leaks; the audit still fails the run on any
     use-after-free, double free or negative count before the trap,
     and on a live object that refers to a freed one at it.
   A rule change that flips a verdict needs the case's header changed
   in the same commit, with the reason.

4. **Adversarial cases are written by someone trying to break it.** A
   separate agent, given only the spec, writes programs intended to be
   accepted yet corrupt memory, or be rejected yet be safe. Each one it
   finds becomes a case.

5. **Generated programs.** A generator produces random well-typed
   programs. Every one the checker accepts must pass the memory audit.
   Failures are minimised and added as cases.

6. **The compiler is checked against the interpreter.** Every case and
   generated program runs both in the reference interpreter and
   compiled through lIR to native code. Results and memory audits must
   match. A feature is done when this passes in CI, not before.

7. **lIR verifies its input.** lIR type-checks whole modules and runs
   the LLVM verifier by default. Invalid lIR is an error with a message,
   never a backend crash or silently wrong code.

## Decision records

Decisions are recorded in the spec chapter they affect, each marked
**Decided** (with the owner's sign-off) or **Proposed**. A proposed
decision can be implemented in the interpreter to test it, but nothing
downstream depends on it until it is decided.
