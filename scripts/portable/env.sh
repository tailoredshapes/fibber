# scripts/portable/env.sh: source it (`. scripts/portable/env.sh`) before running a fibber script by hand on macOS; the Makefile (mk/config.mk) and
# scripts/mac-check.sh do the same. On Linux it changes nothing.
#   PATH      gets scripts/portable/bin first: timeout, flock, nproc, sha256sum, sha1sum, md5sum, sed (GNU `-i`), date (%N), make (gmake), which macOS lacks or has differently
#   BASH_ENV  names bash-env.sh: a `ulimit -v` that succeeds (macOS refuses it)
# Not shimmed, because no shim can: bash 4 features (macOS bash is 3.2), /proc, ldconfig, GNU awk, grep -P. scripts/lint-portable.sh flags those.
if [ "$(uname -s)" = Darwin ]; then
  _portable=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)
  case ":$PATH:" in *":$_portable/bin:"*) ;; *) PATH=$_portable/bin:$PATH ;; esac
  BASH_ENV=$_portable/bash-env.sh
  export PATH BASH_ENV
  [ -n "${BASH_VERSION:-}" ] && . "$_portable/bash-env.sh"   # this shell too, not only the bash scripts it starts
  unset _portable
fi
