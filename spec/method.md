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
     free trace (compiler.md §4). `fibref cases` (the Rust, retired: docs/rust-legacy.md) counted them in the
     interpreter's trace and `fibc cases` in the `FIB_TRACE=1` trace of
     the compiled run; the two traces are the same lines, so the counts
     are the same, except in a threaded run, where the compiled program
     may allocate more, and in a program that makes an `Option` of a
     scalar, where stage 2 holds it as a value and the compiled count is
     lower (compiler.md §8 item 13; the case then carries `;; stage: 2`). More than `N` is a failure, never pending, and
     so is an evaluator that gives no count: a bound nobody checked
     would pass whatever the program did. A `<=` fails only when the
     count goes up, so a bound is worth having when `N` is the count,
     and a case that claims that has a test that lowers `N` by one and
     requires the failure.
     A header may also say `audit: leak-cycle`, a run that leaked and
     erred nowhere, or `audit: abandoned`, the objects a spawned task's
     frames owned when it trapped (a task's trap ends the task and
     abandons them, docs/design/exceptions.md 4.1): both hold when
     something leaked and nothing erred, and `abandoned` must say how
     many with `leaks: N`, which then holds only when exactly `N` leaked
     (so a runtime that frees too much or too little under a trap
     fails). `clean` takes no `leaks`.
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

   Two more header keys say what a case is about and do not change its
   verdict:
   - `covers: NAME ..` (any verdict): the names of the standard
     library's function table (`spec/stdlib.md` §4) that the case calls.
     The test `stdlib_table` fails a row of a delivered tranche that no
     case covers, a name that is no row, and a name that the case's code
     never calls: a `covers:` line alone cannot fail.
   - `open: ITEM ..` (`accept` only): the items (`spec/stdlib.md` §7) whose
     absence is why the case fails today. The case runs. A failure that is
     the program's own (the checker refuses it, it traps, it answers
     differently, the two tools agreeing and the audit clean) is listed as
     OPEN with its items and counted apart: it is not a pass, and it does not
     fail the suite. A pass is a failure, "the item landed: remove `open`",
     so the label cannot outlive its reason; a failure of the tools
     themselves (they disagree, the audit finds an error) stays a failure.

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
