# k-nucleotide

Reads the fasta output (the Java fasta twin's, at size N) from stdin as bytes, takes the sequence after the header `>THREE`
(upper-casing it), counts every k-mer of length 1, 2, 3, 4, 6, 12 and 18 in a hash table, and prints the frequencies of the
1-mers and 2-mers (descending count, then ascending text, percent with three decimals) and the counts of GGT, GGTA, GGTATT, GGTATTTTAATT
and GGTATTTTAATTTATAGT. (The brief of this suite also named k = 24; the published game stops at 18, as these programs do.)

Files: `k-nucleotide.fib`, `.java` (class `k_nucleotide`), `.c`, `.clj` (a script), `sizes.txt`. The input is not an argument: the
programs ignore their arguments and `sizes.txt` gives the N of the fasta output to feed.

## Rules followed
- Same algorithm in all four: the sequence as 2-bit codes in one byte array, one pass per k with a rolling key
  `((key << 2) | code) & mask`, one hash-table update per position, no pre-filtering to the five wanted k-mers.
- fibber: the library's `(Map i64 i64)` and `assoc`/`get` (a loop variable, updated in place), `sort-by` for the tables, `fd-read`
  for stdin. No `unsafe`. Java: `java.util.HashMap<Long,Integer>` with `merge`. Clojure: `java.util.HashMap` with type hints, zero
  reflection warnings, `*unchecked-math*`. C: no hash table in the language, so an open-addressing one (linear probing,
  doubling at half load) written for the twin, which makes C the "what a tuned table costs" column, not a library column.
- The distinct k-mers are few (the generator's LCG has period 139968, so the THREE sequence repeats and the tables stay near
  140 000 entries at any N): the benchmark is lookups and updates of existing keys, not growth.

## Gaps hit
docs/shootout/gaps.md: IO-1 (no byte buffer / array concatenation: the blocks are joined by an element loop), IO-2 (no `format`, and Java's and C's `%.3f` differ on ties: the percentage is computed in exact integer arithmetic in all four), IO-4 (the nested `assoc`/`get` form is 3.4x slower: the program binds the count first).
