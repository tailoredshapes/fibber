# The `fib.db` Driver contract

A driver of `fib.db` is a type that implements the protocol `Connection` (`conn-execute`, `conn-close`).
`fib.db.contract` is the executable statement of what that means: `DriverContract`, a contract in the sense of `fib.test`
(docs/design/test-harness.md), a function from a factory `make` to scenarios. It lives in fibber's library so that a driver in its own
repository (`fib-db-sqlite`, later PostgreSQL) runs the same scenarios against itself, and so that the core does not need a driver to be
tested: `fib.db.memory`, an in-memory fake with no C library, runs the contract in fibber's gate.

## Running it against a driver

```clojure
(ns main (:use fib.core fib.seq fib.coll fib.test.core fib.test.run fib.db fib.db.contract))
(defspecs specs (implements DriverContract "mydriver" (fn () (open-mydriver ":memory:"))))
(defun main () -> i64 (run-main (specs)))
```

`make` takes no arguments and returns a connection to an EMPTY database, one per scenario. The scenarios create their own table
(`people`) and use the SQL that SQLite and PostgreSQL share. Run the file with `fibc test`.

## What it checks

Rows come back as maps keyed by column label, every kind of `Datum` round-tripping (integer, real, text, NULL, bytes, a boolean as 0 or 1);
parameters are bound and never spliced; `execute-one!` is the first row or nil; a statement without columns answers its update count;
`with-transaction` commits on `Ok`, rolls back on `Err`, and rolls back a body that traps and lets the trap go on; errors carry SQLSTATE
(`23505` unique, `23502` not null, class `42` syntax); a closed connection answers `08003` and closing twice is harmless; a connection
works from another task after being used by this one; `plan` walks the rows, stops when the consumer does and gives a failing statement as an Err.

## It can fail

`specs/db-contract-spec.fib` wraps the memory fake in three drivers, an honest one and two with a planted fault (a ROLLBACK that does
nothing; a close that does not close) and asserts, with `failing-scenarios`, the exact scenarios each fault breaks. A driver repository
should do the same once for its own driver (a planted fault in a copy) so that its run of the contract is known to be able to fail.

## The fake

`fib.db.memory` understands only the SQL the contract uses (`create table`, `insert`, `select` with one `where` and one `order by`,
`update`, `delete`, `BEGIN COMMIT ROLLBACK`) and answers 42601 to anything else. It is a test double, not a database: use it to test code
that takes a `Connection`, and the real driver to test SQL. `(memory-closes c)` counts the calls of `conn-close`.
