# fib.merkle: merkle trees

Status: delivered (MERKLE-1, 2026-10-07). Asked for by merkql-fib, the fibber port of merkql (an embedded, Kafka-shaped log whose partitions are
merkle trees), so that the tree is a data structure of the library and the log is built on it.

## What it is

An **incremental** merkle tree over items appended at increasing offsets: merkql's tree (Rust, `src/tree.rs`), byte for byte.

- The tree keeps only its **pending roots**, one per set bit of the count. Appending a leaf merges it with the pending root of equal height, then
  the result with the next, as a binary carry; each merge is a branch. The root folds the pending roots right to left.
- A **node** is a leaf (an item's digest and its offset) or a branch (two digests and the first and last offsets beneath it). Its bytes are
  merkql's, bincode 1: a leaf is u32 0, the item digest, the offset as u64; a branch is u32 1, the left and right digests, then first and last as
  u64, all little-endian. A node's digest is the hasher's digest of its bytes. With SHA-256, a tree here has merkql's root for the same items
  and offsets, and its proofs check against merkql's roots (merkql-fib's scripts/interop.sh checks both directions against the Rust crate).
- An **inclusion proof** carries each sibling's digest, side and offsets, so checking one needs the hasher and nothing else (merkql reads the
  siblings' nodes back from its store to learn their offsets). Proofs from the folded root are the same path merkql walks.

## The interface

```clojure
(:require [fib.merkle :as mk])
(mk/append tree hasher item-digest offset)   ; -> Appended (tree digest written): the new tree, the subtree root, every node made
(mk/root tree hasher)                        ; -> (Option str)
(mk/proof tree hasher lookup offset)         ; -> (Option MerkleProof); lookup: (fn (str) (Option MerkleNode)) over the nodes kept
(mk/verify hasher proof)                     ; -> bool: the item at the offset hashes up to the proof's root
(mk/merkle-log) (mk/log-append l hasher item) (mk/log-root l hasher) (mk/log-proof l hasher offset)   ; nodes kept in a map
(mk/node-bytes n) (mk/node-from-bytes b) (mk/node-hash hasher n)
```

## Decisions

- **The hash is a value**, `(fn ((Array i8)) (Array i8))`: fibber has no cryptography of its own (ADR 0012) and fib.crypto needs a provider.
  Wrap a provider's digest (merkql-fib: `(fn (b) (c/digest p :sha-256 b))`); any function works, which is how case 8320 tests the tree with
  FNV-1a against a separate reference.
- **Digests are lowercase hex `str`.** `(Array i8)` has neither Eq nor Hash, and digests are compared and used as map keys. The bytes are made
  from the hex only when a node is encoded.
- **The tree is a value; the nodes are the caller's.** `append` hands back the nodes it made and the caller keeps them where it likes (a map in
  `MerkleLog`, merkql-fib's pack file). Proofs read them through `lookup`. The pending roots are a plain struct, so persisting a tree is
  persisting `pending` and `count` (merkql's `TreeSnapshot`).
- **Offsets only have to increase.** merkql appends at 0, 1, 2, ...; a tree here may skip (case 8320's sparse tree). An offset that is not after
  the last one traps: it would break the ranges the proofs walk.
- **Not here:** consistency proofs between two roots, deletion, a tree over a fixed set built in one go (append them).

## Evidence

- cases/stdlib/8320: node bytes pinned, roots for 1, 2, 3, 5, 7, 8, 13 and 64 items equal to a reference written separately (Python: the tree as a
  fold of perfect subtrees, largest first, rather than a carry), every offset of a 13-item tree proves, and a proof that lies about the item,
  the offset, a sibling's digest, side or offsets, or the root, fails; node bytes read back; a sparse tree. 8321: the offset order trap.
- scripts/mutant-merkle.sh: 11 planted faults (8 in fib.merkle, 3 in the durability calls of fib.os.files); the cases kill each one.
- merkql-fib (its own repository) certifies its log with a port of merkql's merk-cert suite (27 tests) and reads and writes logs the Rust crate
  wrote, with equal roots and verifying proofs both ways.

## fib.os.files: durability

The log needs four calls fib.os did not have, now in the platform layer (ADR 0011): `sync-fd` (fsync), `sync-path` (open, fsync, close: a
directory too), `rename-file` (rename(2), atomic), `truncate-fd` (ftruncate) and `lock-fd` (flock, exclusive, non-blocking: Err `WouldBlock`
while another open file description holds it). `fib.log.file` used its own `extern rename`; it now calls `rename-file`, and the ADR 0011
allow-list loses that entry. WASI preview 1 has no flock: a program that calls `lock-fd` does not link there (case 8322, which needs mkdtemp too, is listed in
compiler/tests/wasm/expected.txt).
