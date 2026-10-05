#!/bin/bash
# det.sh: bit-reproducibility of a parallel float sum across worker counts (naive chunk-per-worker vs fixed-grain tree), n=1e7, no timing claim
cd /home/tmarsh/.cache/fibber-scratch/par/w
ulimit -v 16000000
echo "naive (chunk = n/W, partials added in task order): ./p4 sumfast 10000000 W 1"
for w in 1 2 3 4 7 8 16 28; do echo "W=$w $(./p4 sumfast 10000000 $w 1 | grep '^v')"; done
echo "fixed grain 65536 + fixed pairwise tree: ./p4 dsumfast 10000000 W 1"
for w in 1 2 3 4 7 8 16 28; do echo "W=$w $(./p4 dsumfast 10000000 $w 1 | grep '^v')"; done
echo "sequential sum-fast: $(./p4 seqsumfast 10000000 1 1 | grep '^v')"
