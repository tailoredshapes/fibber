# scripts/portable/env.sh: source it (`. scripts/portable/env.sh`) before running a fibber script by hand on macOS; the Makefile (mk/config.mk) and
# scripts/mac-check.sh do the same. On Linux it changes nothing.
#   PATH      gets scripts/portable/bin first: timeout, flock, nproc, sha256sum, sha1sum, md5sum, sed (GNU `-i`), date (%N), make (gmake), which macOS lacks or has differently
#   BASH_ENV  names bash-env.sh: a `ulimit -v` that succeeds (macOS refuses it)
# Not shimmed, because no shim can: bash 4 features (macOS bash is 3.2), /proc, ldconfig, GNU awk, grep -P. scripts/lint-portable.sh flags those.
if [ "$(uname -s)" = Darwin ]; then
  _portable=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)
  case ":$PATH:" in *":$_portable/bin:"*) ;; *) PATH=$_portable/bin:$PATH ;; esac
  BASH_ENV=$_portable/bash-env.sh
  # the LLVM 21 libraries of Homebrew's keg-only llvm@21 (the scripts default to Debian's /usr/lib/llvm-21/lib); the PATH of an ssh session lacks /opt/homebrew/bin
  : "${LLVM_LIBDIR:=/opt/homebrew/opt/llvm@21/lib}"; LLVM_LIB=$LLVM_LIBDIR; LLVMLIB=$LLVM_LIBDIR; export LLVM_LIBDIR LLVM_LIB LLVMLIB
  case ":$PATH:" in *":/opt/homebrew/bin:"*) ;; *) [ -d /opt/homebrew/bin ] && PATH=$PATH:/opt/homebrew/bin ;; esac
  export PATH BASH_ENV
  [ -n "${BASH_VERSION:-}" ] && . "$_portable/bash-env.sh"   # this shell too, not only the bash scripts it starts
  unset _portable
fi
