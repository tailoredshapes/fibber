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
#   linked-lo         framedec: a later frame may reach into the first   fast-copy-8      blockdec: the fast path copies 16 bytes at a distance of 8
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$FIBC" ] || { echo "mutant-lz4: no stage 2 fibc: set FIBC" >&2; exit 2; }
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-lz4}
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(last-literals offset-65536 overlap-memmove nibble-overflow ext-255 no-verify match-past-end cap-off-by-one skip-content-crc skip-block-crc
                                     skip-header-crc block-crc-bytes dict-base stale-table hc-depth-zero reserved-bit offset-check fast-offset-0 linked-lo fast-copy-8)

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
    fast-offset-0)    f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(and \(> off 0\) \(<= off \(u- op2 lo\)\)/(and (<= off (u- op2 lo))/' ;;
    linked-lo)        f=$d/framedec.fib; cp $f $f.orig; sub $f 's/\(if first 0 op\) cap/0 cap/' ;;
    fast-copy-8)      f=$d/blockdec.fib; cp $f $f.orig; sub $f 's/\(if \(>= off 16\)(\s+)\(do \(cp16 dp op2 dp/(if (>= off 8)$1(do (cp16 dp op2 dp/' ;;
    *) echo "mutant-lz4: unknown mutant $1" >&2; return 1 ;;
  esac
}

survived=0
for m in "${MUTANTS[@]}"; do
  t=$OUT/$m; rm -rf "$t"; mkdir -p "$t"; cp -r "$R/lib" "$t/lib"; mkdir -p "$t/specs"; cp "$R"/specs/compress-lz4-*.fib "$t/specs/"
  mutate "$m" "$t" || { echo "SETUP ERROR $m"; exit 2; }
  killed=""
  for s in spec edge prop hostile; do
    log=$t/$s.log
    (cd "$t" && FIB_LIB=$t/lib timeout 900 "$FIBC" test specs/compress-lz4-$s*.fib > "$log" 2>&1) && rc=0 || rc=$?
    if [ $rc -ne 0 ] || grep -qE '^  (FAIL|TRAP|ERROR)|scenarios: .* [1-9][0-9]* (fail|trap|timeout)' "$log"; then killed="$s: $(grep -m1 -E '^  (FAIL|TRAP|ERROR)|rejected|trap:' "$log" | cut -c1-110)"; break; fi
  done
  if [ -n "$killed" ]; then echo "killed   $m   ($killed)"; else echo "SURVIVED $m"; survived=1; fi
  rm -rf "$t/lib" "$t/specs"
done
[ $survived = 0 ]
