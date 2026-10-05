#!/bin/bash
# tsan.sh BIN : race reports (unique SUMMARY lines) for several kernels
ulimit -v unlimited
bin=$1
while read -r k n w r; do
  echo "== $k $n $w $r"
  timeout 200 $bin $k $n $w $r 2>&1 | grep -E 'SUMMARY|^v ' | sed 's/(BuildId.*//; s/ThreadSanitizer: //' | sort | uniq -c
done <<'EOF'
sum 200000 4 1
pmap 300 0 1
for 100000 4 1
pfib 22 3 1
asyncjoin 1000 0 1
spawnjoin 200 0 1
EOF
