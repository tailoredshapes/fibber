#!/bin/bash
# linux-only-file: compares the resolver with getent, which macOS does not have (dscacheutil -q host would be the form); not part of the Mac gate
# The differential of the native resolver against the box's own tools, on live names: for each name, the addresses fib.dns returns must be the set `dig +short A` and
# `dig +short AAAA` return (dig follows CNAMEs and prints the addresses after them) when both ask the same server, and for names in /etc/hosts and localhost the set `getent ahosts`
# returns. Needs dig, getent and a network; names that vary per query (round robin, geo DNS) are compared as sets of what both saw over several tries, so a name from a CDN can
# differ legitimately: the default names are stable ones. NAMES="a b" overrides them.
#   FIBC=<fibc> scripts/dns-diff.sh
set -uo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
fibc=${FIBC:?set FIBC}
names=${NAMES:-"example.com www.iana.org localhost no-such-host.invalid"}
for t in dig getent; do command -v "$t" >/dev/null || { echo "dns-diff: $t not found" >&2; exit 2; }; done
ulimit -v 16000000 || true
mine=$("$fibc" run -I "$root/lib" "$root/scripts/dns-resolve.fib" -- $names)
fail=0
for n in $names; do
  got=$(echo "$mine" | sed -n "s/^$n: //p" | tr ' ' '\n' | grep -v '^$' | sort | tr '\n' ' ')
  if [ "$n" = localhost ]; then
    want=$(getent ahosts localhost | awk '{print $1}' | sort -u | tr '\n' ' ')
    # getent lists ::1 and 127.0.0.1; RFC 6761 gives both
    want=$(echo "$want 127.0.0.1 ::1" | tr ' ' '\n' | grep -v '^$' | sort -u | tr '\n' ' ')
  else
    want=$( (dig +short +time=3 +tries=2 A "$n"; dig +short +time=3 +tries=2 AAAA "$n") | grep -E '^[0-9a-fA-F:.]+$' | sort -u | tr '\n' ' ')
    [ -n "$want" ] || want="ERR notfound "
  fi
  if [ "$got" = "$want" ]; then echo "same   $n: $got"; else echo "DIFFER $n: fib.dns=[$got] dig/getent=[$want]"; fail=1; fi
done
exit $fail
