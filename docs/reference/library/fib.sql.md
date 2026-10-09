# fib.sql

Generated declaration inventory of [fib.sql](../../../lib/fib/sql.fib). Source links contain the full signatures and API comments; enum variants and protocol methods belong to their linked declaration.

| Name | Declaration | Defined in |
|---|---|---|
| `Datum` | defenum | [lib/fib/datum.fib:14](../../../lib/fib/datum.fib#L14) |
| `Dialect` | defstruct | [lib/fib/sql/format.fib:8](../../../lib/fib/sql/format.fib#L8) |
| `SqlV` | defenum | [lib/fib/sql/data.fib:7](../../../lib/fib/sql/data.fib#L7) |
| `ToDatum` | defprotocol | [lib/fib/datum.fib:16](../../../lib/fib/datum.fib#L16) |
| `ToSql` | defprotocol | [lib/fib/sql/data.fib:16](../../../lib/fib/sql/data.fib#L16) |
| `add-to-clause` | defun | [lib/fib/sql/data.fib:36](../../../lib/fib/sql/data.fib#L36) |
| `add-where` | defun | [lib/fib/sql/data.fib:47](../../../lib/fib/sql/data.fib#L47) |
| `as-bool` | defun | [lib/fib/datum.fib:31](../../../lib/fib/datum.fib#L31) |
| `as-float` | defun | [lib/fib/datum.fib:29](../../../lib/fib/datum.fib#L29) |
| `as-int` | defun | [lib/fib/datum.fib:28](../../../lib/fib/datum.fib#L28) |
| `as-str` | defun | [lib/fib/datum.fib:30](../../../lib/fib/datum.fib#L30) |
| `binary-ops` | def | [lib/fib/sql/format.fib:27](../../../lib/fib/sql/format.fib#L27) |
| `clause` | defun | [lib/fib/sql/data.fib:26](../../../lib/fib/sql/data.fib#L26) |
| `clause-order` | def | [lib/fib/sql/format.fib:194](../../../lib/fib/sql/format.fib#L194) |
| `datum-json` | defun | [lib/fib/datum.fib:81](../../../lib/fib/datum.fib#L81) |
| `datum-text` | defun | [lib/fib/datum.fib:57](../../../lib/fib/datum.fib#L57) |
| `datum=` | defun | [lib/fib/datum.fib:39](../../../lib/fib/datum.fib#L39) |
| `dialect-of` | defun | [lib/fib/sql.fib:36](../../../lib/fib/sql.fib#L36) |
| `expr` | defun | [lib/fib/sql/format.fib:83](../../../lib/fib/sql/format.fib#L83) |
| `format` | defun | [lib/fib/sql.fib:40](../../../lib/fib/sql.fib#L40) |
| `format-with` | defun | [lib/fib/sql.fib:42](../../../lib/fib/sql.fib#L42) |
| `from` | defmacro | [lib/fib/sql.fib:51](../../../lib/fib/sql.fib#L51) |
| `helper-add` | defmacro | [lib/fib/sql.fib:56](../../../lib/fib/sql.fib#L56) |
| `ident` | defun | [lib/fib/sql/format.fib:10](../../../lib/fib/sql/format.fib#L10) |
| `json-quote` | defun | [lib/fib/datum.fib:69](../../../lib/fib/datum.fib#L69) |
| `keyword-text` | defun | [lib/fib/datum.fib:25](../../../lib/fib/datum.fib#L25) |
| `limit` | defmacro | [lib/fib/sql.fib:54](../../../lib/fib/sql.fib#L54) |
| `order-by` | defmacro | [lib/fib/sql.fib:52](../../../lib/fib/sql.fib#L52) |
| `query-text` | defun | [lib/fib/sql/format.fib:205](../../../lib/fib/sql/format.fib#L205) |
| `select` | defmacro | [lib/fib/sql.fib:50](../../../lib/fib/sql.fib#L50) |
| `sql` | defmacro | [lib/fib/sql.fib:18](../../../lib/fib/sql.fib#L18) |
| `where` | defmacro | [lib/fib/sql.fib:53](../../../lib/fib/sql.fib#L53) |
| `with-clause` | defun | [lib/fib/sql/data.fib:30](../../../lib/fib/sql/data.fib#L30) |
