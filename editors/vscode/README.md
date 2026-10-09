# fibber for VS Code

Syntax highlighting, bracket and comment handling, and snippets for fibber (`.fib`).
The grammar has no build step; the language client needs `npm install` (below).

Install without publishing: copy or symlink this directory to
`~/.vscode/extensions/tailoredshapes.fibber-0.2.0` and reload the window.

Highlighted: `;` and `#_` comments, strings with escapes, characters, integers (widths, `0x`, `0b`, `_`),
floats, ratios, keywords, `true false nil`, definition names, special forms, the library's macros,
`x:` annotations, `->`, capitalised type names, `ns/name` aliases, reader macros (`@ & ~ ~@ ' \` #(`).

## Language server (completion, hover, diagnostics, go to definition, document symbols)

The extension starts the language server of the compiler, `fibc lsp` (`lsp-main` of `compiler/lsp/server.fib`;
the Rust `fibref lsp` predecessor is retired), through `vscode-languageclient`. The protocol is plain LSP
over standard input and output with Content-Length framing, full-text sync and no custom messages. There is
no bundler: `extension.js` is plain JS and the one npm dependency is the client library.

```
cd editors/vscode
npm install
```

then install the directory as above (the `node_modules` directory must be there). You need a `fibc` with `lsp`
(build it with `fibc build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o fibc`, or
use a release that has the command) on the `PATH`, or set the path below.

- **Completion** (triggers `(`, `/`, `.`, `:` and space): locals and parameters in scope, the file's own
  definitions, the library with the checker's scheme as detail, special forms and macros, `alias/` exports,
  the fields after `(. x `, the types after `x: ` and `->`, the file's keywords after `:`. It works while the
  buffer is broken: the scope degrades from the checked program to the reader's, then to the library.
- **Hover**: the checker's scheme for a global, the type for a local (when the buffer checks).
- **Diagnostics**: the front end's errors, refreshed on every edit.
- **Go to definition**: a local or parameter, a top-level definition of the file, a field after `(. x `, and
  any name of the program (`alias/name` too) in the library or another module, in the file that defines it.
- **Document symbols**: the file's `defun`, `defstruct`, `defenum`, `defprotocol`, `def` and macros (Outline,
  breadcrumbs, Go to Symbol).

Settings: `fibber.serverPath` (default `fibc`; the server runs as `<path> lsp`), `fibber.fibrefPath` (the old
name, empty by default; when set it is used instead), `fibber.libraryPath` (becomes `FIB_LIB` for the server;
empty means the library the server carries), `fibber.includePaths` (each becomes `-I`). A change to any of them
restarts the server; so does the command "fibber: Restart the language server". If the executable is
missing there is one status-bar warning (click it to restart after fixing the path) and nothing else:
no output channel and no toasts.

Tests: `npm test` spawns the real server and runs an initialize, didOpen (broken buffer, one diagnostic),
completion, didChange, hover, definition, document symbols, an unknown method, shutdown and exit round trip
over stdio. The server is `$FIBREF` (a `fibref`), else `$FIBC` (a `fibc`), else `fibc` on the `PATH`, else
the Rust build at `target/debug/fibref`. **With none of them the test fails**: a test that passes when there
is nothing to test cannot fail. `FIBREF_SKIP=1 npm test` skips on purpose (it prints `SKIPPED` and exits 0).
The recorded transcripts of the Rust server are replayed by `compiler/tests/lsp/server.sh`.

The real client: `npm run test:host` (`test/host.js`) downloads a VS Code with `@vscode/test-electron` into
`$VSCODE_CACHE` (default `~/.cache/fibber-vscode`, about 330 MB, nothing is installed on the system), starts it
under `xvfb-run` when there is no display, loads this extension (so the `vscode-languageclient` in `node_modules`
is the client) and, in the extension host (`test/host/index.js`), opens a `.fib` file and asks through the
VS Code commands for document symbols, hover, definition, completion and diagnostics, then edits the file and
waits for the diagnostics to clear. Install the runner first with `npm install --no-save @vscode/test-electron`
(`package.json` and the lockfile do not carry it). `FIBC` names the server; it exits 2, in words, when the
runner or a display is missing. A server that does not start fails the test after 60 s.
