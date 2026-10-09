# fib.db

Generated declaration inventory of [fib.db](../../../lib/fib/db.fib). Source links contain the full signatures and API comments; enum variants and protocol methods belong to their linked declaration.

| Name | Declaration | Defined in |
|---|---|---|
| `Connection` | defprotocol | [lib/fib/db/core.fib:24](../../../lib/fib/db/core.fib#L24) |
| `Datum` | defenum | [lib/fib/datum.fib:14](../../../lib/fib/datum.fib#L14) |
| `DbError` | defstruct | [lib/fib/db/core.fib:13](../../../lib/fib/db/core.fib#L13) |
| `Query` | defstruct | [lib/fib/db/core.fib:20](../../../lib/fib/db/core.fib#L20) |
| `ToDatum` | defprotocol | [lib/fib/datum.fib:16](../../../lib/fib/datum.fib#L16) |
| `as-bool` | defun | [lib/fib/datum.fib:31](../../../lib/fib/datum.fib#L31) |
| `as-float` | defun | [lib/fib/datum.fib:29](../../../lib/fib/datum.fib#L29) |
| `as-int` | defun | [lib/fib/datum.fib:28](../../../lib/fib/datum.fib#L28) |
| `as-str` | defun | [lib/fib/datum.fib:30](../../../lib/fib/datum.fib#L30) |
| `call-with-connection` | defun | [lib/fib/db/core.fib:71](../../../lib/fib/db/core.fib#L71) |
| `col` | defun | [lib/fib/db/core.fib:38](../../../lib/fib/db/core.fib#L38) |
| `datum-json` | defun | [lib/fib/datum.fib:81](../../../lib/fib/datum.fib#L81) |
| `datum-text` | defun | [lib/fib/datum.fib:57](../../../lib/fib/datum.fib#L57) |
| `datum=` | defun | [lib/fib/datum.fib:39](../../../lib/fib/datum.fib#L39) |
| `db-error?` | defun | [lib/fib/db/core.fib:17](../../../lib/fib/db/core.fib#L17) |
| `execute!` | defmacro | [lib/fib/db.fib:17](../../../lib/fib/db.fib#L17) |
| `execute-one!` | defmacro | [lib/fib/db.fib:26](../../../lib/fib/db.fib#L26) |
| `execute-one-query` | defun | [lib/fib/db/core.fib:44](../../../lib/fib/db/core.fib#L44) |
| `execute-query` | defun | [lib/fib/db/core.fib:40](../../../lib/fib/db/core.fib#L40) |
| `json-quote` | defun | [lib/fib/datum.fib:69](../../../lib/fib/datum.fib#L69) |
| `keyword-text` | defun | [lib/fib/datum.fib:25](../../../lib/fib/datum.fib#L25) |
| `plan` | defmacro | [lib/fib/db.fib:49](../../../lib/fib/db.fib#L49) |
| `plan-query` | defun | [lib/fib/db/core.fib:87](../../../lib/fib/db/core.fib#L87) |
| `transact` | defun | [lib/fib/db/core.fib:63](../../../lib/fib/db/core.fib#L63) |
| `update-count` | defun | [lib/fib/db/core.fib:50](../../../lib/fib/db/core.fib#L50) |
| `with-connection` | defmacro | [lib/fib/db.fib:43](../../../lib/fib/db.fib#L43) |
| `with-transaction` | defmacro | [lib/fib/db.fib:36](../../../lib/fib/db.fib#L36) |
