# Platform support and evidence

The compiler's `targets` table describes code generation capability, not a
release/test promise. Its [generated snapshot](../reference/targets.md) is checked
against the current compiler. Proposed 1.0 platform tiers await owner approval.

| Target | Current evidence / boundary | Build or check |
|---|---|---|
| x86_64 Linux GNU | Native default gate; AVX2/FMA, glibc 2.33+ | `make gate` |
| aarch64 Darwin | Published arm64 release; separate native gate | `gmake mac-check` |
| aarch64 Linux GNU | Cross-emission; native node gate available | `make k8s-gate K8S_ARCH=arm64` |
| x86_64 Linux musl | Static release/program checks when musl is present | `make musl`, `make static` |
| aarch64 Linux musl | Cross-emission and recorded qemu-user runs | `--static --target aarch64-unknown-linux-musl` |
| arm64 iOS | Emission only; no recorded iOS execution | SDK/static-library linking; not JIT |
| wasm32-wasip1 | WASI programs run under Node/wasmtime; no threads/FMA | `make fetch-wasm`, `make wasm` |
| wasm32-unknown-unknown | Bare wasm; imports/host glue needed | `--target wasm32-unknown-unknown` |
| riscv64 Linux | Parked: LLVM has no tailcc | refused unless `--allow-unsupported` |
| nvptx64 NVIDIA | PTX kernel emission; external CUDA driver | `--emit ptx` |
| wgsl WebGPU | WGSL kernel emission; external WebGPU driver | `--emit wgsl` |

Use `FIB_TARGET_CPU=x86-64-v3` when building x86 binaries to distribute.
Unset/host builds can require your build host's newer instructions. Cross-linking
requires the target's linker, libraries and sysroot. C and wasm retain their
explicit portability limits; enabling `--allow-unsupported` is not support evidence.
