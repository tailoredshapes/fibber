# fibber for VS Code

Syntax highlighting, bracket and comment handling, and snippets for fibber (`.fib`).
Plain TextMate grammar: no build step, no node dependency, nothing to run.

Install without publishing: copy or symlink this directory to
`~/.vscode/extensions/tailoredshapes.fibber-0.3.0` and reload the window.

Highlighted: `;` and `#_` comments, strings with escapes, characters, integers (widths, `0x`, `0b`, `_`),
floats, ratios, keywords, `true false nil`, definition names, special forms, the library's macros,
`x:` annotations, `->`, capitalised type names, `ns/name` aliases, reader macros (`@ & ~ ~@ ' \` #(`).

## No language server (since 0.3.0)

Versions 0.2.x started `fibref lsp` for completion, hover and diagnostics. `fibref` was the Rust reference interpreter, which is
gone (ROADMAP stage 10, docs/rust-legacy.md), and it could not read the current library any more. The extension therefore starts
no server and has no setting that names an executable: it is the grammar, the language configuration and the snippets, and
those work as before. Completion, hover and diagnostics are not available.

The planned replacement is a language server in the compiler itself, `fibc lsp` (docs/design/dev-loop.md): it will run the same
front end that `fibc` runs, so the editor and the compiler cannot disagree. When it exists the extension will start it again
(the client of 0.2.x is in git history: `editors/vscode/extension.js` before the commit that removed it, and its protocol is
spec/bootstrap.md section 9).
