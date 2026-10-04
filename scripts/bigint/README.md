# scripts/bigint

The oracle of the BigInt library's differential cases (`cases/stdlib/6000-6002`).

- `BigDiff.java` draws seeded operands with tl.rng's xorshift64 (the same generator as `tl.bigdiff`), computes fourteen
  kinds of operation with `java.math.BigInteger` and prints one digest per kind: an xorshift step per byte of every
  result's decimal text, in order. `cases/stdlib/support/tl/bigdiff.fib` does the same with fibber's BigInt; the cases
  embed the digests, so the cases need no JVM.
- Regenerate after changing the operand generator or the kinds (both files change together):
  `javac -d DIR scripts/bigint/BigDiff.java && java -cp DIR BigDiff`, then paste the digests into the three cases.
- Find the first differing result of a kind: `java -cp DIR BigDiff trace K` against a fibber program that calls
  `(bigdiff-run K true)`; diff the two outputs.
- A planted fault must fail the digests: the mutations tried when the library was written (the add-back step of
  Algorithm D, the Karatsuba middle term, the rounding of a negative right shift, the borrow of subtraction, the sign of
  a sum) were each reported by the kinds that exercise them; two mutations were equivalent programs (the retry of the
  quotient estimate, which the add-back step makes redundant, and the width at which a long operand is cut into pieces).
