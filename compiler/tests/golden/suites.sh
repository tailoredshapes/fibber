# compiler/tests/golden/suites.sh: the golden suites, sourced by golden.sh and record-from-rust.sh (not run).
# One line of `suite_table` per suite: NAME | STAGE2 TOOL | OPTIONS | RUST COMMAND | INPUT FILES (shell globs, repository-relative).
#   STAGE2 TOOL  the tool built from compiler/TOOL.fib (read, expand, types, own, explain, emit); invoked as `TOOL OPTIONS FILE`.
#   RUST COMMAND what the Rust oracle of tag seed-1 was run as, `BIN SUBCOMMAND` (docs/rust-legacy.md); used by record-from-rust.sh only.
# The golden of a suite is NAME.golden (the output and exit status of the tool on each input, in order) and NAME.files (the inputs in it).
# Two suites have inputs whose outputs are large (the dump of every token of 1187 reader inputs, of 949 module programs after expansion: 100 MB of text);
# their golden holds the SHA-256 of each input's output and exit status instead of the text. A failure names the input; `golden.sh --show SUITE FILE`
# prints stage 2's output for it, and the Rust's text is in docs/rust-legacy.md's way of resurrecting the oracle.
HASHED=" reader-dump expand-porter "
# shape NAME BLOCKFILE FILE: what the golden holds for one input whose block (the `#### FILE` line, the output, the `status N` line) is in BLOCKFILE
shape() {
  if [[ "$HASHED" == *" $1 "* ]]; then echo "#### $3 $(tail -n +2 "$2" | sha256sum | cut -d' ' -f1)"; else cat "$2"; fi
}
suite_table() {
  cat <<'EOF'
reader-dump|read||fibref read|compiler/tests/reader/*.fib
reader-print|read|--print|fibref read|compiler/tests/reader/*.fib
expand-e1|expand|--no-runner --|fibref expand|compiler/tests/expand/e1-*.fib compiler/tests/expand/m2b-*.fib
expand-r|expand|--no-runner --|fibref expand|compiler/tests/expand/r[0-9]*.fib compiler/tests/expand/x[0-9]*.fib
expand-porter|expand|--no-runner --|fibref expand|compiler/tests/expand/porter[0-9]-*.fib
types-infer|types||fibref types|compiler/tests/types/infer-cases/*.fib
types-top-sections|types|--stage lower --sections type,protocol,instance,fun,def,extern,error|fibref types|compiler/tests/types/top-cases/*.fib
types-top-ast|types|--stage lower --ast|fibref types|compiler/tests/types/top-cases/*.fib
own-amp|own||fibref own|compiler/tests/own/amp-cases/*.fib
own-taken|own|--sections taken,error|fibref own|compiler/tests/own/taken-cases/*.fib
explain-amp|explain||fibref explain|compiler/tests/own/amp-cases/*.fib
emit-fns|emit|--sections fns|fibc emit-dump|compiler/tests/emit/[a-z]*-*.fib
emit-defs|emit|--sections statics,defs,fns|fibc emit-dump|compiler/tests/emit/def-*.fib
lair-ast|lairf|dump-ast|lair|cases/lir/*/*.lir compiler/tests/native/h2-edge/*.lir
lair-check|lairf|check|lair|cases/lir/*/*.lir compiler/tests/native/h2-edge/*.lir
lair-llvm|lairf|emit-llvm|lair|cases/lir/*/*.lir
EOF
}
