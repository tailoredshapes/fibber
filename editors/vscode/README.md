# fibber for VS Code

Syntax highlighting, bracket and comment handling, and snippets for fibber (`.fib`).
Plain TextMate grammar: no build step.

Install without publishing: copy or symlink this directory to
`~/.vscode/extensions/tailoredshapes.fibber-0.1.0` and reload the window.

Highlighted: `;` and `#_` comments, strings with escapes, characters, integers (widths, `0x`, `0b`, `_`),
floats, ratios, keywords, `true false nil`, definition names, special forms, the library's macros,
`x:` annotations, `->`, capitalised type names, `ns/name` aliases, reader macros (`@ & ~ ~@ ' \` #(`).

## Language server (completion, hover, diagnostics)

Since 0.2.0 the extension starts `fibref lsp` (protocol: spec/bootstrap.md section 9) through
`vscode-languageclient`. There is no bundler: `extension.js` is plain JS and the one npm dependency is
the client library.

```
cd editors/vscode
npm install
```

then install the directory as above (the `node_modules` directory must be there). You need a `fibref`
that has `lsp`: `cargo build -p fibref` and put `target/debug/fibref` on `PATH`, or set the path below.

- **Completion** (triggers `(`, `/`, `.`, `:` and space): locals and parameters in scope, the file's own
  definitions, the library with the checker's scheme as detail, special forms and macros, `alias/` exports,
  the fields after `(. x `, the types after `x: ` and `->`, the file's keywords after `:`. It works while the
  buffer is broken: the scope degrades from the checked program to the reader's, then to the library.
- **Hover**: the checker's scheme for a global, the type for a local (when the buffer checks).
- **Diagnostics**: the front end's errors, refreshed on every edit.

Settings: `fibber.fibrefPath` (default `fibref`), `fibber.libraryPath` (becomes `FIB_LIB` for the server;
empty means the library fibref carries), `fibber.includePaths` (each becomes `-I`). A change to any of them
restarts the server; so does the command "fibber: Restart the language server". If the executable is
missing there is one status-bar warning (click it to restart after fixing the path) and nothing else:
no output channel and no toasts.

Tests: `npm test` spawns the real `fibref lsp` (from `$FIBREF`, `$CARGO_TARGET_DIR/debug/fibref` or
`../../target/debug/fibref`; skipped with a message when there is none) and runs an initialize, didOpen
(broken buffer, one diagnostic), completion, didChange, hover, shutdown and exit round trip over stdio.
