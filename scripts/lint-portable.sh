#!/bin/bash
# scripts/lint-portable.sh: no Linux-only tool or bash 4 feature in the scripts that macOS runs (package MAC-1). macOS ships bash 3.2, BSD sed/awk/grep/date/stat/tar
# and none of GNU coreutils' extras; a script that uses one breaks on the Mac silently (`sed -i -E` made a backup file and did not edit; `date +%s.%N` printed a letter).
# The rule (a text check, on purpose cheap): a line of scripts/, mk/, Makefile or compiler/tests/**/*.sh that matches a pattern of the table below is a finding unless
#   - the line (or the line above it) carries `# linux-only: REASON`  (a reviewed Linux-only use: the reason is required; the tool list of the Mac,
#     compiler/tests/expected-macos.txt, says which tools skip because of such a line), or
#   - the file carries `# linux-only-file: REASON` in its first 25 lines (a whole script that is Linux-only), or
#   - the tool is one that scripts/portable/bin shims for macOS (timeout, flock, nproc, sha1sum, sha256sum, md5sum, `sed -i`, `date +%N`, `ulimit -v`): not in the table.
# Comment lines (first non-blank character `#`) are not checked. Not checked, because a text check cannot: an empty array under `set -u` ("${a[@]}" is an error in bash 3.2),
# `sed` escapes that BSD sed lacks in a basic regex (\+ \| \? are found by the one heuristic below), `\t` or `\n` in a sed replacement.
# usage: scripts/lint-portable.sh [ROOT]      checks ROOT (default: this tree), then the selftest
#        scripts/lint-portable.sh --selftest  only the plants: every rule must flag its planted line, and the markers must be honoured (and a marker without a reason refused)
# exit 0 when clean, 1 for findings or a failed selftest. Wired into the tools stage (mk/tools.mk: lint-portable), as lint-pipefail is.
set -u
here=$(cd "$(dirname "$0")" && pwd)

# The rules, one per line: NAME, ERE, portable replacement, and an optional ERE that excuses the line, TAB separated; @C@ stands for "command position".
# (BSD awk and mawk read POSIX EREs only: no \b, \s, \d.)
rules() {
  C='(^|[;&|(){`@]|[$][(]|then|do|else|exec|sudo|nice|xargs|env)[[:space:]]*'
  awk -v c="$C" 'BEGIN { gsub(/&/, "\\\\&", c) } { gsub("@C@", c); print }' <<'EOF'
mapfile	@C@(mapfile|readarray)([[:space:]]|$)	bash 3.2 has no mapfile: while IFS= read -r line; do a+=("$line"); done < <(...)
declare-A	(declare|local|typeset)[[:space:]]+-[a-zA-Z]*A[[:space:]]	bash 3.2 has no associative arrays: a case statement, or parallel arrays
wait-n	(^|[;&|({[:space:]])wait[[:space:]]+-n([[:space:]]|$|;)	bash 3.2 has no wait -n: wait for the oldest pid, or poll with kill -0
case-mod	[$][{][A-Za-z_0-9]+(,,|\^\^|,|\^)[}]	bash 3.2 has no ${x,,} or ${x^^}: tr '[:upper:]' '[:lower:]'
append-both	&>>	bash 3.2 has no &>>: >> file 2>&1
pipe-stderr	[|]&([[:space:]]|$)	bash 3.2 has no |&: 2>&1 |
test-v	\[\[[[:space:]]+-v[[:space:]]	bash 3.2 has no [[ -v var ]]: [ -n "${var+x}" ]
coproc	@C@coproc[[:space:]]	bash 3.2 has no coproc
negative-index	[A-Za-z_]\[-[0-9]+\]	bash 3.2 has no negative array subscripts: ${a[${#a[@]}-1]}
proc	/proc/[a-z0-9]	macOS has no /proc: sysctl or ps, or mark the line linux-only
sysfs	(/sys/(devices|class|fs|kernel)|/dev/shm)	macOS has no /sys or /dev/shm: mark the line linux-only
ldconfig	@C@ldconfig([[:space:]]|$)	macOS has no ldconfig: mark linux-only, or find the library by path
ldd	@C@ldd([[:space:]]|$)	macOS has no ldd: otool -L	otool
readelf	@C@readelf([[:space:]]|$)	macOS has no readelf: otool -l or otool -L
taskset	@C@taskset([[:space:]]|$)	macOS has no taskset or CPU pinning: mark linux-only
linux-tool	@C@(numactl|lscpu|setarch|chrt|ionice|unshare|nsenter|setcap|chattr|fallocate|stdbuf|setsid|getent|strace|ltrace|truncate|tac|shuf)([[:space:]]|$)	Linux-only tool (not in macOS): mark linux-only, or find a portable form
getopt	@C@getopt[[:space:]]	BSD getopt has no long options: a while/case loop
grep-P	grep[[:space:]]+(-[a-zA-Z]*P|--perl-regexp)	BSD grep has no -P: grep -E, or awk, or perl -ne
stat-c	@C@stat[[:space:]]+(-c|--format|--printf)	BSD stat has -f: wc -c < file for a size, or python3
date-d	@C@date[[:space:]]+(-d|--date)	BSD date has -j -f and -v: compute with python3 or awk
readlink	@C@readlink[[:space:]]+(-[a-eg-z]*f|--canonicalize)	readlink -f needs macOS 12.3: (cd "$(dirname f)" && pwd)
mktemp-gnu	@C@mktemp[[:space:]]+(--|-t[[:space:]]|-p[[:space:]])	BSD mktemp differs: mktemp -d "${TMPDIR:-/tmp}/name.XXXXXX"
tar-gnu	@C@tar[[:space:]].*--(transform|wildcards|owner=|group=|exclude-vcs|sort=)	bsdtar has no such option: tar then mv, or python3 tarfile
long-option	@C@(cp|mv|ln|rm|ls|du|sort|cut|head|tail|od|basename|dirname)[[:space:]]([^|;&]*[[:space:]])?--(reflink|color|time-style|no-preserve-root|complement|zero-terminated|files0-from|parents|suffix|parallel|buffer-size|random-sort)	GNU long option: not on macOS
du-b	@C@du[[:space:]]+-[a-zA-Z]*b	BSD du has no -b: wc -c, or du -k
base64-w	@C@base64[[:space:]]+(-w|--wrap)	BSD base64 has no -w: base64 | tr -d newline
head-neg	@C@head[[:space:]]+-[cn][[:space:]]*-[0-9]	BSD head has no negative counts: sed '$d', or awk
find-gnu	@C@find[[:space:]].*-(printf|regextype)	BSD find has no -printf or -regextype: -exec stat, or python3
xargs-d	@C@xargs[[:space:]].*(-d[[:space:]]|--delimiter)	BSD xargs has no -d: tr '\n' '\0' | xargs -0
sed-gnu	@C@sed[[:space:]]+(-[a-zA-Z]*z|--null-data|--debug|--follow-symlinks)	BSD sed has no -z or long options
sed-bre-escape	@C@sed[[:space:]][^|]*['"][^'"]*\\[+|?]	BSD sed's basic regex has no \+ \| \?: sed -E with + | ?	sed[[:space:]]+(-[a-zA-Z]*[Er])
awk-gnu	(strtonum|gensub|asorti?[(]|PROCINFO|systime[(]|strftime[(]|mktime[(]|IGNORECASE)	GNU awk only (macOS awk is the one-true-awk): plain awk, or python3
time-v	(/usr/bin/time|@C@time)[[:space:]]+-(v|f|o)[[:space:]]	BSD time has -l, not GNU -v -f -o: mark linux-only, or bash's time
ps-gnu	@C@ps[[:space:]].*(--ppid|--sort|--no-headers|--forest)	BSD ps has other options: ps -o ... -p PID
sort-gnu	@C@sort[[:space:]]+-[a-zA-Z]*[Rz]([[:space:]]|$)	BSD sort has no -R or -z: awk with rand(), or python3
dd-gnu	@C@dd[[:space:]].*(iflag=|oflag=|status=|conv=fdatasync)	BSD dd lacks iflag oflag status: mark linux-only, or plain dd
EOF
}

# lint FILE..: prints one `file:line: [rule] text` per finding
lint() {
  local rf; rf=$(mktemp "${TMPDIR:-/tmp}/lint-portable.rules.XXXXXX") || return 1
  rules > "$rf"
  awk -v rf="$rf" '
    BEGIN { FS = "\t"; nr = 0; while ((getline line < rf) > 0) { split(line, f, "\t"); nr++; name[nr] = f[1]; re[nr] = f[2]; ex[nr] = f[4] } FS = " " }
    FNR == 1 { prev = ""; filemark = 0 }
    FNR <= 25 && /# linux-only-file:[[:space:]]*[^[:space:]]/ { filemark = 1 }
    {
      l = $0
      if (l ~ /# linux-only/ && l !~ /# linux-only(-file)?:[[:space:]]*[^[:space:]]/) print FILENAME ":" FNR ": [marker] `# linux-only` needs a reason: `# linux-only: why`"
      ok = (l ~ /# linux-only:[[:space:]]*[^[:space:]]/ || prev ~ /^[[:space:]]*#[[:space:]]*linux-only:[[:space:]]*[^[:space:]]/)
      if (l !~ /^[[:space:]]*#/ && !filemark && !ok) {
        for (i = 1; i <= nr; i++) if (l ~ re[i] && (ex[i] == "" || l !~ ex[i])) { print FILENAME ":" FNR ": [" name[i] "] " l; break }
      }
      prev = l
    }
  ' "$@" 2>&1 | sort
  rm -f "$rf"
}

selftest() {
  local d bad=0 got n=0 rule line; d=$(mktemp -d "${TMPDIR:-/tmp}/lint-portable.XXXXXX") || return 1
  # one planted line per rule: the rule must flag it
  while IFS='|' read -r rule line; do
    [ -n "$rule" ] || continue
    printf '#!/bin/bash\n%s\n' "$line" > "$d/plant.sh"; n=$((n + 1))
    got=$(lint "$d/plant.sh")
    case $got in *"[$rule]"*) ;; *) echo "lint-portable selftest: the plant for $rule was not flagged: $line (got: $got)" >&2; bad=1 ;; esac
  done <<'PLANTS'
mapfile|mapfile -t names < <(ls)
declare-A|declare -A want=([a]=1)
wait-n|while [ "$(jobs -rp | wc -l)" -ge 4 ]; do wait -n; done
case-mod|x=${name,,}
append-both|run &>> log.txt
pipe-stderr|run |& tee log.txt
test-v|if [[ -v HOME ]]; then :; fi
coproc|coproc SERVER { server; }
negative-index|last=${a[-1]}
proc|grep -q avx2 /proc/cpuinfo
sysfs|cat /sys/devices/system/cpu/online
ldconfig|lib=$(ldconfig -p | head -1)
ldd|ldd ./prog
readelf|readelf -d ./prog
taskset|taskset -c 3 ./prog
linux-tool|setsid ./prog &
getopt|getopt -o a -l long -- "$@"
grep-P|grep -oP 'a\Kb' f
stat-c|size=$(stat -c %s f)
date-d|d=$(date -d yesterday +%F)
readlink|r=$(readlink -f "$0")
mktemp-gnu|t=$(mktemp --suffix=.fib)
tar-gnu|tar --transform 's,^,x/,' -cf a.tar d
long-option|ls --color=never
du-b|n=$(du -sb dir)
base64-w|base64 -w0 file
head-neg|head -n -1 file
find-gnu|find . -printf '%p\n'
xargs-d|ls | xargs -d '\n' echo
sed-gnu|sed -z 's/a/b/' f
sed-bre-escape|sed 's/a\+/b/' f
awk-gnu|awk '{ print strtonum($1) }' f
time-v|/usr/bin/time -v ./prog
ps-gnu|ps --ppid 1 -o pid
dd-gnu|dd if=/dev/zero of=f bs=1M count=1 status=none
sort-gnu|sort -R f
PLANTS
  # the markers: honoured with a reason (same line, line above, whole file), refused without one
  printf '#!/bin/bash\ngrep -q avx2 /proc/cpuinfo   # linux-only: x86 feature flags\n' > "$d/m1.sh"
  printf '#!/bin/bash\n# linux-only: the pool is read from /proc\ncat /proc/self/status\n' > "$d/m2.sh"
  printf '#!/bin/bash\n# linux-only-file: ThreadSanitizer is a Linux tool\ncat /proc/self/status\n' > "$d/m3.sh"
  printf '#!/bin/bash\ngrep -q avx2 /proc/cpuinfo   # linux-only\n' > "$d/m4.sh"
  printf '#!/bin/bash\n# cat /proc/self/status is only in a comment\nx=$(sed -i s/a/b/ f; timeout 5 true; sha256sum f; nproc; flock 9; ulimit -v 1000; date +%%s.%%N)\n' > "$d/clean.sh"
  printf 'x:\n\t@mapfile -t a < <(ls)\n' > "$d/plant.mk"
  for f in m1 m2 m3 clean; do
    got=$(lint "$d/$f.sh"); n=$((n + 1))
    [ -z "$got" ] || { echo "lint-portable selftest: $f.sh was flagged: $got" >&2; bad=1; }
  done
  got=$(lint "$d/m4.sh"); n=$((n + 1))
  case $got in *"[marker]"*) ;; *) echo "lint-portable selftest: a linux-only marker without a reason was not refused (got: $got)" >&2; bad=1 ;; esac
  got=$(lint "$d/plant.mk"); n=$((n + 1))
  case $got in *"[mapfile]"*) ;; *) echo "lint-portable selftest: the planted Makefile recipe was not flagged (got: $got)" >&2; bad=1 ;; esac
  rm -rf "$d"
  [ $bad -eq 0 ] && echo "lint-portable selftest: $n plants behave (each rule flags its line; markers honoured with a reason, refused without one; the shimmed tools pass)"
  return $bad
}

[ "${1:-}" = "--selftest" ] && { selftest; exit $?; }
root=${1:-$(cd "$here/.." && pwd)}
files=()
while IFS= read -r f; do files+=("$f"); done < <({ find "$root/scripts" "$root/compiler/tests" -type f -name '*.sh' ! -name lint-portable.sh ! -path "$root/scripts/portable/*"; find "$root/Makefile" "$root/mk" -type f \( -name Makefile -o -name '*.mk' \); } 2> /dev/null | sort)
[ ${#files[@]} -gt 20 ] || { echo "lint-portable: fewer than 20 scripts found under $root (a search of nothing finds nothing)" >&2; exit 1; }
found=$(lint "${files[@]}")
rc=0
if [ -n "$found" ]; then echo "$found" | sed "s|^$root/||"; for r in $(echo "$found" | sed -n 's/^[^[]*\[\([a-z0-9A-Z-]*\)\].*/\1/p' | sort -u); do rules | awk -F "\t" -v r="$r" '$1 == r { print "  " r ": " $3 }'; done; echo "lint-portable: $(echo "$found" | wc -l | tr -d ' ') use(s) of a Linux-only tool or a bash 4 feature (scripts/lint-portable.sh says how to fix or mark each)"; rc=1
else echo "lint-portable: ${#files[@]} scripts, no Linux-only tool or bash 4 feature outside a reviewed linux-only mark"; fi
selftest || rc=1
exit $rc
