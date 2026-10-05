#!/bin/bash
# a64.sh: cross-compile the lIR deque and its mutants to arm64 objects, copy them with sched.c to the Mac scratch dir, run the stress there
cd /home/tmarsh/.cache/fibber-scratch/par/w
L=/home/tmarsh/.cache/fibber-scratch/EXC/lairf
for m in "" .m1 .m2 .m3 .m4; do
  $L build cl$m.lir --target arm64-apple-macosx -o cla$m.o -O 2 --emit obj 2>&1 | head -3
done
ls cla*.o
ssh 192.168.7.254 'mkdir -p ~/fibber-a64-scratch/par'
scp -q cla*.o sched.c 192.168.7.254:fibber-a64-scratch/par/
ssh 192.168.7.254 'cd ~/fibber-a64-scratch/par && cc -O2 -o sched sched.c -lpthread 2>&1 | head -3; for m in "" .m1 .m2 .m3 .m4; do cc cla$m.o -o cla$m.exe -lm -lpthread 2>&1 | head -2; echo "== cl$m on M1 Ultra (20 runs)"; for i in $(seq 1 20); do ./cla$m.exe | tail -1; done | sort | uniq -c | sort -rn | head -4; done'
