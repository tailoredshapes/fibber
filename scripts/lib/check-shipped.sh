#!/bin/bash
# scripts/lib/check-shipped.sh BIN PLATFORM CPU LLVM_LINK: the checks of the shipped fibc (mk/release.mk, build/release/binary.ok; they were
# the middle of scripts/package.sh). With LLVM_LINK static: Linux: no RUNPATH/RPATH, no NEEDED liblair or libLLVM, `ldd` shows only libc, libm,
# libstdc++, libgcc_s, libz and libzstd (and the loader); no AVX-512 in the code when CPU is x86-64-v3 (x86-64-v3 has AVX2 and FMA, not AVX-512;
# LLVM's static archives carry BLAKE3's AVX-512 kernels, picked at run time by cpuid, so `_llvm_blake3_*_avx512` and only those are allowed);
# the start-up CPU check (docs/adr/0008, glibc's __x86_get_cpuid_feature_leaf) is in it. macOS (`otool`): no LC_RPATH, every dylib under
# /usr/lib or /System, a valid signature. With LLVM_LINK shared the binary needs the LLVM shared library and these checks are skipped (a note).
set -eu
bin=${1:?usage: check-shipped.sh BIN PLATFORM CPU LLVM_LINK}; plat=${2:?PLATFORM}; cpu=${3:?CPU}; link=${4:?LLVM_LINK}
if [ "$link" = static ] && [ "$plat" = darwin-arm64 ]; then
  if otool -l "$bin" | grep -q LC_RPATH; then
    echo "check-shipped: the shipped fibc has an LC_RPATH (the cc shim did not take):" >&2; otool -l "$bin" | grep -A2 LC_RPATH >&2; exit 1
  fi
  stray=$(otool -L "$bin" | tail -n +2 | awk '{print $1}' | grep -v -E '^(/usr/lib/|/System/)' || true)
  if [ -n "$stray" ]; then echo "check-shipped: the shipped fibc loads libraries outside /usr/lib and /System: $stray" >&2; exit 1; fi
  codesign -v "$bin" || { echo "check-shipped: the shipped fibc has no valid signature" >&2; exit 1; }
  echo "shipped fibc: no LC_RPATH, loads: $(otool -L "$bin" | tail -n +2 | awk '{print $1}' | tr '\n' ' ')"
elif [ "$link" = static ]; then
  if readelf -d "$bin" | grep -q -E 'RUNPATH|RPATH'; then   # linux-only: the ELF branch of a Linux release; the darwin branch above uses otool
    echo "check-shipped: the shipped fibc has an RUNPATH/RPATH (the cc shim did not take, or a library is still found by one):" >&2
    readelf -d "$bin" | grep -E 'RUNPATH|RPATH' >&2; exit 1   # linux-only: the ELF branch of a Linux release; the darwin branch above uses otool
  fi
  if readelf -d "$bin" | grep NEEDED | grep -q -E 'liblair|libLLVM'; then   # linux-only: the ELF branch of a Linux release; the darwin branch above uses otool
    echo "check-shipped: the shipped fibc needs liblair or libLLVM:" >&2; readelf -d "$bin" | grep NEEDED >&2; exit 1   # linux-only: the ELF branch of a Linux release; the darwin branch above uses otool
  fi
  extra=$(ldd "$bin" | awk '{print $1}' | sed 's|.*/||' | grep -v -E '^(linux-vdso|ld-linux)|^(libc|libm|libstdc\+\+|libgcc_s|libz|libzstd)\.' || true)   # linux-only: the ELF branch of a Linux release; the darwin branch above uses otool
  if [ -n "$extra" ]; then echo "check-shipped: ldd shows libraries beyond libc, libm, libstdc++, libgcc_s, libz, libzstd: $extra" >&2; exit 1; fi   # linux-only: the ELF branch of a Linux release; the darwin branch above uses otool
  echo "shipped fibc: no RUNPATH, NEEDED: $(readelf -d "$bin" | sed -n 's/.*Shared library: \[\(.*\)\]/\1/p' | tr '\n' ' ')"   # linux-only: the ELF branch of a Linux release; the darwin branch above uses otool
else
  echo "note: LLVM_LINK=shared: this binary needs the LLVM shared library; it is not a release"
fi
if [ "$plat" = linux-x86_64 ] && command -v objdump >/dev/null 2>&1 && [ "$cpu" = x86-64-v3 ]; then
  avx512=$(objdump -d --no-show-raw-insn "$bin" | awk '/^[0-9a-f]+ <.*>:$/ { fn = $2 } /zmm[0-9]|[{]%k[1-7][}]/ { c[fn]++ } END { for (f in c) print f }' \
        | grep -v -E '^<_llvm_blake3_[a-z_]*avx512>:$' || true)
  if [ -n "$avx512" ]; then
    echo "check-shipped: the shipped fibc uses AVX-512 although it was built for $cpu (FIB_TARGET_CPU did not take), in: $(echo "$avx512" | head -n 5 | tr '\n' ' ')" >&2; exit 1
  fi
  echo "shipped fibc: no AVX-512 outside BLAKE3's own kernels; $(objdump -d --no-show-raw-insn "$bin" | grep -c -E 'vfmadd|vfnmadd|vfmsub') FMA instructions"
fi
if [ "$plat" = linux-x86_64 ]; then
  nm -D "$bin" | grep -q __x86_get_cpuid_feature_leaf || { echo "check-shipped: the shipped fibc has no start-up CPU check (docs/adr/0008)" >&2; exit 1; }
  echo "shipped fibc: the start-up CPU check is in it"
fi
