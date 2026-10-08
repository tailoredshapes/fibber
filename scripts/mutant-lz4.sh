#!/bin/bash
# scripts/mutant-lz4.sh: planted faults in fib.compress.lz4 (docs/design/compress.md 8). Copies lib/ of this tree to a scratch directory, applies ONE mutant, runs the lz4 specs
# (specs/compress-lz4-spec.fib, -edge-, -prop-, then -hostile- only if those held) with a stage 2 (FIBC; default the gate's F of this tree) pointing FIB_LIB at the copy: a spec must
# FAIL. A mutant under which every spec passes is a part no test can fail.
# usage: scripts/mutant-lz4.sh [MUTANT..]        (default: all)
#   last-literals     enc: the last 5 bytes may be matched            offset-65536       enc: a match at distance 65536 is allowed
#   overlap-memmove   mem: an overlapping match is copied in chunks   nibble-overflow    enc: a literal length above 14 overflows the token's nibble
#   ext-255           enc: a length extension is cut at 256, not 255   no-verify          enc: a hash candidate is taken without comparing its bytes
#   match-past-end    enc: a match may extend over the last 5 bytes    cap-off-by-one     dec: one byte more than the cap is accepted
#   skip-content-crc  framedec: the content checksum is not compared   skip-block-crc     framedec: a block checksum is not compared
#   skip-header-crc   header: the header checksum is not compared      block-crc-bytes    frame: the block checksum covers the size word too
#   dict-base         frame: the dictionary sits one byte off in the window   stale-table  scomp: independent blocks keep the table and reach ahead
#   hc-depth-zero     hc: the chain is not followed                    reserved-bit       header: the reserved FLG bit is accepted
#   offset-check      blockdec: an offset one past the output is accepted   fast-offset-0  blockdec: the fast path does not check for offset 0
#   slack-dlim / slack-ilim   blockdec: the fast path's output / input slack is too small (killed by the safety spec's canaries)   fast-small-pattern   blockdec: small offsets use a distance of 8
#   xxh-vector-rot    xxh32: the vector round rotates by 14           par-order  par-race-window  par-lost-job  par-max-output  par-thread-dependent  par-error-swallowed  par-trap-kills  par-checksum-skipped (see docs/design/compress.md 9)
#   linked-lo         framedec: a later frame may reach into the first   fast-copy-8      blockdec: the fast path copies 16 bytes at a distance of 8
#   mid-islack mid-dslack  blockdec: the medium path's input / output slack is too small   mid-offset  the medium path checks only for offset 0   mid-pattern  small offsets use a distance of 8
#   hc-opt-price  hcopt: a sequence costs one byte more   hc-pa-off  hc3: no pattern analysis   hc-swap-off  no chain swap   hc-opt-full  level 12 does not search everywhere   hc-opt-skip  the skip test is strict
#   par-nested-threads  par: runners are OS threads again (a call inside a pmap body multiplies them)   par-unbounded-flight  par: one runner per job, whatever `threads` says
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$FIBC" ] || { echo "mutant-lz4: no stage 2 fibc: set FIBC" >&2; exit 2; }
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-lz4}
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(slack-dlim slack-ilim fast-small-pattern xxh-vector-rot par-order par-race-window par-lost-job par-max-output par-thread-dependent par-error-swallowed par-trap-kills par-checksum-skipped last-literals offset-65536 overlap-memmove nibble-overflow ext-255 no-verify match-past-end cap-off-by-one skip-content-crc skip-block-crc
                                     skip-header-crc block-crc-bytes dict-base stale-table hc-depth-zero reserved-bit offset-check fast-offset-0 linked-lo fast-copy-8
                                     mid-islack mid-dslack mid-offset mid-pattern par-nested-threads par-unbounded-flight
                                     hc-opt-price hc-pa-off hc-swap-off hc-opt-full hc-opt-skip)

sub() { perl -0pi -e "$2" "$1"; cmp -s "$1" "$1.orig" && { echo "mutant-lz4: pattern not found in $1: $2" >&2; return 1; }; return 0; }
mutate() { # mutate NAME TREE
  local d=$2/lib/fib/compress/lz4 f
  case $1 in
    last-literals)    f=$d/blockenc.fib; cp $f $f.orig; sub $f 's/\(def last-lits: i64 5\)/(def last-lits: i64 4)/' ;;
    offset-65536)     f=$d/blockenc.fib; cp $f $f.orig; sub $f 's/\(def max-dist: i64 65535\)/(def max-dist: i64 65536)/' ;;
    overlap-memmove)  f=$d/mem.fib; cp $f $f.orig; sub $f 's/\(>= off 8\) \(copy-far d di off n 8/(>= off 1) (copy-far d di off n 8/' ;;
    nibble-overflow)  f=$d/blockenc.fib; cp $f $f.orig; sub $f 's/\(shl \(min lit 15\) 4\)/(shl (min lit 16) 4)/' ;;
    ext-255)          f=$d/blockenc.fib; cp $f $f.orig; sub $f 's/full \(quot n 255\)/full (quot n 256)/' ;;
    no-verify)        f=$d/blockenc.fib; cp $f $f.orig; sub $f 's/ \(= \(rd32 sp cand\) \(rd32 sp ip\)\)\)\)/ true))/' ;;
    match-past-end)   f=$d/blockenc.fib; cp $f $f.orig; sub $f 's/mlim \(u- iend last-lits\)/mlim iend/' ;;
    cap-off-by-one)   f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(> lit \(u- cap op\)\) e-too-large/(> lit (u- (u+ cap 1) op)) e-too-large/' && sub $f 's/\(> ml \(u- cap op2\)\) e-too-large/(> ml (u- (u+ cap 1) op2)) e-too-large/' ;;
    skip-content-crc) f=$d/framedec.fib; cp $f $f.orig; sub $f 's/\(and \(\. h cchk\) \(not \(= \(rd32 \(data src\)/(and false (not (= (rd32 (data src)/' ;;
    skip-block-crc)   f=$d/framedec.fib; cp $f $f.orig; sub $f 's/\(and \(\. h bchk\) \(not \(= \(rd32 p/(and false (not (= (rd32 p/' ;;
    skip-header-crc)  f=$d/header.fib; cp $f $f.orig; sub $f 's/\(not \(= \(rd8 p \(\+ pos \(\+ 4/(and false (= (rd8 p (+ pos (+ 4/' ;;
    block-crc-bytes)  f=$d/frame.fib; cp $f $f.orig; sub $f 's/\(xxh32-ptr \(unsafe \(ptr\+ dp \(u?\+ op 4\)\)\) len 0\)/(xxh32-ptr (unsafe (ptr+ dp op)) len 0)/' ;;
    dict-base)        f=$d/frame.fib; cp $f $f.orig; sub $f 's/\(copy-exact \(data w\) \(array-len dict\) \(data src\)/(copy-exact (data w) (+ 1 (array-len dict)) (data src)/' ;;
    stale-table)      f=$d/scomp.fib; cp $f $f.orig; sub $f 's/indep \(rs r 4 \(\+ \(rg r 4\) \(\+ n 1\)\)\)/indep ()/' && { f=$d/blockenc.fib; cp $f $f.orig; sub $f 's/\(< cand ip\) //'; } ;;
    hc-depth-zero)    f=$d/hc.fib; cp $f $f.orig; sub $f 's/\(cond \(<= level 3\) 4/(cond (<= level 99) 0 (<= level 3) 4/' ;;
    reserved-bit)     f=$d/header.fib; cp $f $f.orig; sub $f 's/\(> \(bit-and flg 2\) 0\) \(Err \(corrupt "the frame.s reserved FLG bit is set"\)\)/false (Err (corrupt "x"))/' ;;
    offset-check)     f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(> off \(u- op2 lo\)\)\) e-corrupt/(> off (u+ (u- op2 lo) 1))) e-corrupt/' ;;
    fast-offset-0)    f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(ult \(u- \(rd16 ip \(u\+ \(u\+ i 1\) lit\)\) 1\) \(u- \(u\+ op lit\) lo\)\)/(<= (rd16 ip (u+ (u+ i 1) lit)) (u- (u+ op lit) lo))/' ;;
    linked-lo)        f=$d/framedec.fib; cp $f $f.orig; sub $f 's/\(if first 0 op\) cap/0 cap/' ;;
    fast-copy-8)      f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(cond \(>= off 16\) \(do \(cp16 dp op2 dp/(cond (>= off 8) (do (cp16 dp op2 dp/' ;;
    fast-small-pattern) f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(let \[s \(period8 off\) src/(let [s 8 src/' ;;
    slack-dlim)       f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(u- dlen 46\)/(u- dlen 30)/' ;;
    slack-ilim)       f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(u- ilen 17\)/(u- ilen 10)/' ;;
    xxh-vector-rot)   f=$d/xxh32.fib; cp $f $f.orig; sub $f 's/\(shl a \(vs 13\)\) \(shr a \(vs 19\)\)/(shl a (vs 14)) (shr a (vs 18))/' ;;
    par-order)        f=$d/pframe.fib; cp $f $f.orig; sub $f 's/blocks \(vec \(drop 1 parts\)\)/blocks (vec (reverse (drop 1 parts)))/' ;;
    par-race-window)  f=$d/pframedec.fib; cp $f $f.orig; sub $f 's/ro \(\* k \(\. s bmax\)\)/ro (max 0 (- (* k (. s bmax)) 1))/' ;;
    par-lost-job)     f=$d/../par.fib; cp $f $f.orig; sub $f 's/\(if \(>= i n\) out/(if (>= i (- n 1)) out/' ;;
    par-max-output)   f=$d/pframedec.fib; cp $f $f.orig; sub $f 's/\(> \(\. h csize\) \(\. o max-output\)\) \(> \(\. h csize\)/false (> (. h csize)/' ;;
    par-thread-dependent) f=$d/pframe.fib; cp $f $f.orig; sub $f 's/e \(enc-new \(\. o level\) \(\. o acceleration\) n\)/e (enc-new (. o level) (+ (. o acceleration) (if (= j 1) 1 0)) n)/' ;;
    par-error-swallowed) f=$d/pframedec.fib; cp $f $f.orig; sub $f 's/\(if \(= \(\. e kind\) :internal\) \(Err e\) \(Ok nil\)\)/(if (= (. e kind) :internal) (Err e) (Ok (some (array 0 0i8))))/' ;;
    par-trap-kills)   f=$d/../par.fib; cp $f $f.orig; sub $f 's/\(Err \(internal \(str "a worker trapped: " \(\. e message\)\)\)\)/(trap "a worker trapped")/' ;;
    par-checksum-skipped) f=$d/pframedec.fib; cp $f $f.orig; sub $f 's/\(and \(\. o verify\) \(\. h cchk\)\)/false/' ;;
    mid-islack)       f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(> \(u\+ i2 32\) ilenm\) 0/(> (u+ i2 8) ilenm) 0/' ;;
    mid-dslack)       f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(> \(u\+ end 32\) dlenm\)/(> (u+ end 8) dlenm)/' ;;
    mid-offset)       f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(not \(ult \(u- off 1\) \(u- op2 lo\)\)\)\) 0/(= off 0)) 0/' ;;
    mid-pattern)      f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(far8 dp \(u\+ op2 24\) \(period8 off\) end\)/(far8 dp (u+ op2 24) 8 end)/' ;;
    par-nested-threads) f=$d/../par.fib; cp $f $f.orig; sub $f 's/\(fork-task \(fn \(\) \(work next failed n job\)\)\)/(spawn (fn () (work next failed n job)))/' ;;
    par-unbounded-flight) f=$d/../par.fib; cp $f $f.orig; sub $f 's/\(parallel \(min workers n\) n job\)/(parallel n n job)/' ;;
    hc-opt-price)     f=$d/hcopt.fib; cp $f $f.orig; sub $f 's/\(u\+ \(u\+ 3 \(lit-price ll\)\)/(u+ (u+ 4 (lit-price ll))/' ;;
    hc-pa-off)        f=$d/hc3.fib; cp $f $f.orig; sub $f 's/pa \(or \(= mode 1\) \(> \(\. c depth\) 128\)\)/pa false/' ;;
    hc-swap-off)      f=$d/hc3.fib; cp $f $f.orig; sub $f 's/\(and \(= mode 1\) \(= \(u\+ fwd back\) best2\)/(and (= mode 2) (= (u+ fwd back) best2)/' ;;
    hc-opt-full)      f=$d/hcopt.fib; cp $f $f.orig; sub $f 's/\(>= \(\. c level\) 12\)/(>= (. c level) 13)/' ;;
    hc-opt-skip)      f=$d/hcopt.fib; cp $f $f.orig; sub $f 's/\(<= \(og c \(u\+ cur 1\) 0\) \(og c cur 0\)\)\)\)/(< (og c (u+ cur 1) 0) (og c cur 0))))/' ;;
    *) echo "mutant-lz4: unknown mutant $1" >&2; return 1 ;;
  esac
}

survived=0
for m in "${MUTANTS[@]}"; do
  t=$OUT/$m; rm -rf "$t"; mkdir -p "$t"; cp -r "$R/lib" "$t/lib"; mkdir -p "$t/specs"; cp "$R"/specs/compress-lz4-*.fib "$t/specs/"
  mutate "$m" "$t" || { echo "SETUP ERROR $m"; exit 2; }
  killed=""
  for s in spec edge prop hc safety xxh par hostile; do
    log=$t/$s.log
    (cd "$t" && FIB_LIB=$t/lib timeout 900 "$FIBC" test specs/compress-lz4-$s*.fib > "$log" 2>&1) && rc=0 || rc=$?
    if [ $rc -ne 0 ] || grep -qE '^  (FAIL|TRAP|ERROR)|scenarios: .* [1-9][0-9]* (fail|trap|timeout)' "$log"; then killed="$s: $(grep -m1 -E '^  (FAIL|TRAP|ERROR)|rejected|^  unsupported' "$log" | cut -c1-110)"; break; fi
  done
  if [ -n "$killed" ]; then echo "killed   $m   ($killed)"; else echo "SURVIVED $m"; survived=1; fi
  rm -rf "$t/lib" "$t/specs"
done
[ $survived = 0 ]
