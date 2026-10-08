#!/bin/bash
# scripts/lint-pipefail.sh: no `set -o pipefail` script pipes a producer into a consumer that exits early (package FOLLOWUP-1). With pipefail, a pipeline
# `produce | head -n1`, `produce | grep -q x` or `produce | awk '{print; exit}'` fails when the consumer leaves before the producer has written everything:
# the producer is killed by SIGPIPE (status 141) and the whole pipeline reports failure, silently, on exactly the inputs that are long (a log over 64 KiB, a
# test run that prints many lines). The fix is to read the producer's output into a variable first (`out=$(produce) || true`) and then search the variable,
# or to give the consumer a here-string (`grep -q x <<< "$out"`), or to end the producer's side with `|| true` when its status does not matter.
#
# The rule (a text check, on purpose cheap): in a script that mentions `pipefail` outside a comment, a pipeline whose consumer is `head`, `grep -q`/`-qE`,
# `awk ... exit` or `sed ... q` is a finding unless
#   - the pipeline starts with `echo` or `printf` (a shell builtin writes its text in one go, which the pipe buffer takes before the consumer can leave), or
#   - the pipeline is followed by `|| true` (or `|| :`: its status is thrown away), or
#   - the line carries the marker `# pipe-ok` (a reviewed exception: say why on the line).
# Since MAKE-1 the Makefile and mk/*.mk are checked too: every recipe runs under `bash -eu -o pipefail` (mk/config.mk), so a recipe line is a pipefail line.
# usage: scripts/lint-pipefail.sh [ROOT]      checks ROOT/scripts, ROOT/compiler/tests, ROOT/.github, ROOT/Makefile and ROOT/mk (default: this tree), then the selftest
#        scripts/lint-pipefail.sh --selftest   only the plant: the rule must flag a planted `producer | head -1` and must pass the builtin and the marked forms
# exit 0 when clean, 1 for findings or a failed selftest. Called by scripts/tools.sh (the `lint-pipefail` entry of the gate).
set -u
here=$(cd "$(dirname "$0")" && pwd)

# lint FILE..: prints one `file:line: text` per finding
lint() {
  awk '
    { lines[FILENAME, FNR] = $0 }
    /pipefail/ && $0 !~ /^[ \t]*#/ { pfile[FILENAME] = 1 }
    FILENAME ~ /(^|\/)Makefile$|\.mk$/ { pfile[FILENAME] = 1 }   # every recipe runs under the pipefail of mk/config.mk
    { n[FILENAME] = FNR }
    END {
      for (f in n) {
        if (!(f in pfile)) continue
        for (i = 1; i <= n[f]; i++) {
          l = lines[f, i]
          if (l ~ /^[ \t]*#/ || l ~ /# pipe-ok/) continue
          gsub(/\|\|/, ";;", l)   # `||` is not a pipe: a separator, as `;` is
          rest = l; off = 0
          while (match(rest, /\|[ \t]*(head([ \t]|$|\)|`)|grep[ \t]+(-[a-zA-Z]+[ \t]+)*-[a-zA-Z]*q|awk[ \t][^|]*exit|sed[ \t][^|]*[;'"'"'" ]q([ ;}'"'"'" ]|$))/)) {
            left = substr(l, 1, off + RSTART - 1)
            # the first stage of this pipeline: after the last command separator before the consumer
            k = 0
            inq = 0
            for (p = length(left); p >= 1; p--) {
              c = substr(left, p, 1); c2 = substr(left, p - 1, 2)
              if (c == "\047") { inq = !inq; continue }   # a single quote: what it holds is text, not a command
              if (inq) continue
              if (c == "(" || c == "{" || c == ";" || c == "`" || c2 == "&&" || c2 == "||" ) { k = p; break }
              if (substr(left, p, 5) == "then ") { k = p + 4; break }
            }
            first = substr(left, k + 1); sub(/^[ \t&|!]+/, "", first)
            while (first ~ /^(if|elif|while|until|then|else|do)[ \t]/) { sub(/^[a-z]+[ \t]+/, "", first); sub(/^[ \t!]+/, "", first) }
            after = substr(l, off + RSTART + RLENGTH)
            guarded = (after ~ /^[^;|]*;;[ \t]*(true|:)([ \t;})]|$)/)   # `produce | head -1 || true`: the status is thrown away
            if (!guarded && first !~ /^(echo|printf)[ \t]/ && first !~ /^\(?(echo|printf)[ \t]/) { print f ":" i ": " l; break }
            off += RSTART + RLENGTH - 1; rest = substr(l, off + 1)
          }
        }
      }
    }
  ' "$@" 2>/dev/null | sort
}

selftest() {
  local d; d=$(mktemp -d) || return 1
  printf '#!/bin/bash\nset -uo pipefail\nline=$(produce | head -1)\n' > "$d/bad-head.sh"
  printf '#!/bin/bash\nset -euo pipefail\nif produce | grep -q x; then echo y; fi\n' > "$d/bad-grep.sh"
  printf '#!/bin/bash\nset -o pipefail\nx=$(produce | awk '"'"'{print; exit}'"'"')\n' > "$d/bad-awk.sh"
  printf '#!/bin/bash\nset -uo pipefail\nok=$(echo "$out" | head -1)\nif printf "%%s" "$out" | grep -q x; then :; fi\nl=$(produce | head -1) # pipe-ok: one line\n' > "$d/good.sh"
  printf '#!/bin/bash\nproduce | head -1\n' > "$d/no-pipefail.sh"
  printf 'x:\n\t@produce | head -1\n' > "$d/bad-recipe.mk"
  local bad=0 got
  for f in bad-head.sh bad-grep.sh bad-awk.sh bad-recipe.mk; do
    got=$(lint "$d/$f")
    [ -n "$got" ] || { echo "lint-pipefail selftest: the planted $f.sh was not flagged" >&2; bad=1; }
  done
  for f in good no-pipefail; do
    got=$(lint "$d/$f.sh")
    [ -z "$got" ] || { echo "lint-pipefail selftest: $f.sh was flagged: $got" >&2; bad=1; }
  done
  rm -rf "$d"
  [ $bad -eq 0 ] && echo "lint-pipefail selftest: 4 planted pipelines flagged (one a Makefile recipe), 2 clean files passed"
  return $bad
}

[ "${1:-}" = "--selftest" ] && { selftest; exit $?; }
root=${1:-$(cd "$here/.." && pwd)}
files=()
while IFS= read -r f; do files+=("$f"); done < <({ find "$root/scripts" "$root/compiler/tests" "$root"/.github -type f \( -name '*.sh' -o -name '*.yml' \) ! -name lint-pipefail.sh; find "$root/Makefile" "$root/mk" -type f \( -name Makefile -o -name '*.mk' \); } 2>/dev/null | sort)
[ ${#files[@]} -gt 20 ] || { echo "lint-pipefail: fewer than 20 scripts found under $root (a search of nothing finds nothing)" >&2; exit 1; }
found=$(lint "${files[@]}")
rc=0
if [ -n "$found" ]; then echo "$found"; echo "lint-pipefail: $(echo "$found" | wc -l) pipeline(s) end in a consumer that exits early under pipefail (scripts/lint-pipefail.sh says how to fix)"; rc=1
else echo "lint-pipefail: ${#files[@]} scripts, no early-exit consumer under pipefail"; fi
selftest || rc=1
exit $rc
