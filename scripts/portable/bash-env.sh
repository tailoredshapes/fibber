# scripts/portable/bash-env.sh: BASH_ENV for macOS (bash reads it at the start of every non-interactive bash, so scripts at any depth see it).
# macOS has no RLIMIT_AS: `ulimit -v N` fails there ("cannot modify limit: Invalid argument"), and in `ulimit -v N && cmd` or under `set -e` that stops the
# script. The limit only guards a Linux runaway; here it is a no-op that succeeds.
ulimit() {
  case "${1:-}" in
    -v|-d|-m) builtin ulimit "$@" 2> /dev/null || true ;;
    *) builtin ulimit "$@" ;;
  esac
}
