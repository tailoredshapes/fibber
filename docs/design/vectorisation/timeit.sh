#!/bin/bash
# usage: timeit.sh NAME [N REPS] : runs o/NAME.exe over all modes, best of 3 ms, holding the shared lock once for the whole batch
exec flock /tmp/fibsuite.lock bash /home/tmarsh/.cache/fibber-scratch/SIMD-V/k/timeinner.sh "$@"
