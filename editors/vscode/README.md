# fibber for VS Code

Syntax highlighting, bracket and comment handling, and snippets for fibber (`.fib`).
Plain TextMate grammar: no build step, no dependencies.

Install without publishing: copy or symlink this directory to
`~/.vscode/extensions/tailoredshapes.fibber-0.1.0` and reload the window.

Highlighted: `;` and `#_` comments, strings with escapes, characters, integers (widths, `0x`, `0b`, `_`),
floats, ratios, keywords, `true false nil`, definition names, special forms, the library's macros,
`x:` annotations, `->`, capitalised type names, `ns/name` aliases, reader macros (`@ & ~ ~@ ' \` #(`).
