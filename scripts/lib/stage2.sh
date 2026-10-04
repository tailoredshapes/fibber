# scripts/lib/stage2.sh: shared by scripts/gate.sh, scripts/bench/quick.sh and scripts/batch.sh (sourced, not run).
# Builds stage 2 (compiler/fibc.fib, "F") and the stage-3 fixed point, caching both under $GATE_OUT keyed by a stamp of
# everything they are built from, so a run whose compiler/ and lib/ are unchanged builds nothing.
#   root          the repository (a worktree) this is run for
#   GATE_OUT      scratch for F, F3, stamps, case output; default ~/.cache/fibber-scratch/gate-<name of root>
#   FIBC | SEED   a fibc to build with (the Rust seed, or any stage 2); SEED may also name a directory holding bin/fibc or fibc
#   LLVM_LINK     how stage 2 links LLVM: `shared` (default: `-L /usr/lib/llvm-21/lib -l LLVM-21`, LLVM_LIBDIR names another directory) or `static`
#                 (the archives, through the ld script scripts/llvm-static.sh writes under $GATE_OUT)
#   LAIR_DIR      only for a builder that is the Rust fibc or a release before the flip (it loads liblair.so itself): the directory with liblair.so;
#                 default: the builder's directory if it has one, else target/debug of the main checkout; a builder with none is fine (a stage 2 needs
#                 no liblair: it links LLVM)
#   GATE_FRESH=1  never build from the previous F
# Without FIBC or SEED: the previous F ($GATE_OUT/F) if it was built from the same lib/prelude.fib (the prelude is embedded in a
# compiler, so a changed one needs the seed), else the Rust fibc of target/debug, else bin/fibc, else `fibc` on PATH.
root=${root:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}
GATE_OUT=${GATE_OUT:-$HOME/.cache/fibber-scratch/gate-$(basename "$root")}
mkdir -p "$GATE_OUT/tmp"
export TMPDIR=$GATE_OUT/tmp FIB_LIB=$root/lib

now() { date +%s.%N; }
elapsed() { echo "$2 - $1" | bc | awk '{printf "%.1f", $1}'; }

# The files a stage 2 is built from, as one hash (tests and notes of compiler/ are not inputs).
tree_stamp() {
  (cd "$root" && find compiler lib -type f -not -path 'compiler/tests/*' -not -path 'compiler/mirror-pending/*' | LC_ALL=C sort \
     | xargs sha1sum | sha1sum | cut -c1-16)
}
prelude_stamp() { sha1sum < "$root/lib/prelude.fib" | cut -c1-16; }

# Sets BUILDER (the fibc to build with), BUILDER_NOTE (a sentence saying which) and LAIR_DIR.
pick_builder() {
  local s=${FIBC:-${SEED:-}} main_target
  main_target=$(cd "$root" && git rev-parse --git-common-dir 2>/dev/null | sed 's|/\.git$||')/target/debug
  if [ -n "$s" ]; then
    if [ -d "$s" ]; then if [ -x "$s/bin/fibc" ]; then s=$s/bin/fibc; else s=$s/fibc; fi; fi
    BUILDER=$s; BUILDER_NOTE="from the seed you named ($s)"
  elif [ -z "${GATE_FRESH:-}" ] && [ -x "$GATE_OUT/F" ] && [ "$(cat "$GATE_OUT/F.prelude" 2>/dev/null)" = "$(prelude_stamp)" ]; then
    BUILDER=$GATE_OUT/F; BUILDER_NOTE="from the previous F ($GATE_OUT/F, same prelude)"
  elif [ -x "$root/target/debug/fibc" ]; then BUILDER=$root/target/debug/fibc; BUILDER_NOTE="from the Rust fibc of this tree"
  elif [ -x "$main_target/fibc" ]; then BUILDER=$main_target/fibc; BUILDER_NOTE="from the Rust seed ($main_target/fibc)"
  elif [ -x "$root/bin/fibc" ]; then BUILDER=$root/bin/fibc; BUILDER_NOTE="from bin/fibc"
  else BUILDER=$(command -v fibc || true); BUILDER_NOTE="from fibc on PATH"; fi
  [ -x "${BUILDER:-}" ] || { echo "stage2.sh: no fibc to build with: set FIBC or SEED" >&2; return 2; }
  if [ -z "${LAIR_DIR:-}" ]; then
    for d in "$(dirname "$BUILDER")" "$(dirname "$BUILDER")/../lib" "${LD_LIBRARY_PATH:-/nonexistent}" "$main_target" "$root/target/debug"; do
      if [ -e "$d/liblair.so" ]; then LAIR_DIR=$d; break; fi
    done
  fi
  # The builder may be a seed that compiles with its own liblair.so (found above, or next to it in a release's lib/); a stage 2 has none.
  if [ -e "${LAIR_DIR:-/nonexistent}/liblair.so" ]; then export LD_LIBRARY_PATH=$LAIR_DIR; else unset LD_LIBRARY_PATH; fi
  llvm_link_args
}

# Sets LLVM_ARGS: the `-L`/`-l` words that link LLVM into a stage 2, by LLVM_LINK (shared or static).
llvm_link_args() {
  local libdir=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
  case ${LLVM_LINK:-shared} in
    shared) LLVM_ARGS=(-L "$libdir" -l LLVM-21) ;;
    static) read -r -a LLVM_ARGS < <("$root/scripts/llvm-static.sh" "$GATE_OUT/llvm-static") || return 2 ;;
    *) echo "stage2.sh: LLVM_LINK is shared or static, not ${LLVM_LINK}" >&2; return 2 ;;
  esac
}

build_with() { # build_with FIBC OUT: stage 2 from the tree with FIBC
  (cd "$root" && "$1" build compiler/fibc.fib -I compiler -I lib "${LLVM_ARGS[@]}" -o "$2")
}

# stage2_ensure: F built from this tree at $GATE_OUT/F (cached by stamp). Prints what it did.
stage2_ensure() {
  local st t0; st=$(tree_stamp); t0=$(now)
  F=$GATE_OUT/F
  pick_builder || return 2
  if [ -x "$F" ] && [ "$(cat "$F.stamp" 2>/dev/null)" = "$st" ]; then
    echo "build: F cached (compiler/ and lib/ unchanged, stamp $st)"; return 0
  fi
  echo "build: stage 2 $BUILDER_NOTE"
  rm -f "$F.new"
  if ! build_with "$BUILDER" "$F.new" > "$GATE_OUT/build.log" 2>&1; then tail -n 20 "$GATE_OUT/build.log"; echo "build: FAILED"; return 1; fi
  mv "$F.new" "$F"; echo "$st" > "$F.stamp"; prelude_stamp > "$F.prelude"; rm -f "$GATE_OUT/F3" "$GATE_OUT/F3.stamp"
  echo "build: built in $(elapsed "$t0" "$(now)") s"
}

emit_with() { (cd "$root" && "$1" emit -I compiler -I lib compiler/fibc.fib); }

# stage3_check: F builds F3, and F and F3 emit the same lIR (the fixed point of CI's stage check). Cached by stamp.
stage3_check() {
  local st t0 p1; st=$(tree_stamp); t0=$(now); F=$GATE_OUT/F
  if [ "$(cat "$GATE_OUT/F3.stamp" 2>/dev/null)" = "$st" ]; then echo "fixed point: cached (stamp $st)"; return 0; fi
  emit_with "$F" > "$GATE_OUT/emit.F" 2> "$GATE_OUT/emit.F.err" &
  p1=$!
  if ! build_with "$F" "$GATE_OUT/F3" > "$GATE_OUT/build3.log" 2>&1; then
    tail -n 20 "$GATE_OUT/build3.log"; wait; echo "fixed point: F could not build F3"; return 1
  fi
  wait "$p1" || { echo "fixed point: F could not emit"; return 1; }
  emit_with "$GATE_OUT/F3" > "$GATE_OUT/emit.F3" 2> "$GATE_OUT/emit.F3.err" || { echo "fixed point: F3 could not emit"; return 1; }
  if cmp -s "$GATE_OUT/emit.F" "$GATE_OUT/emit.F3"; then
    echo "$st" > "$GATE_OUT/F3.stamp"; echo "fixed point: F and F3 emit the same lIR ($(elapsed "$t0" "$(now)") s)"
  else echo "fixed point: F and F3 emit DIFFERENT lIR"; return 1; fi
}

# The lock every heavy run takes (a gate run, a bench run), so a bench never overlaps a gate. Held by the process that took it and its
# children (FIBSUITE_LOCKED says so, so a script that calls another does not wait on itself).
take_suite_lock() {
  [ -n "${FIBSUITE_LOCKED:-}" ] && return 0
  exec 9> /tmp/fibsuite.lock
  flock -n 9 || { echo "waiting for /tmp/fibsuite.lock (another gate or bench is running)"; flock 9; }
  export FIBSUITE_LOCKED=1
}
