# Compiler targets

Generated from `fibc targets`; see [platform evidence](../policy/platform-support.md) before deployment.

| triple | status | support |
|---|---|---|
| x86_64-unknown-linux-gnu | runs | supported |
| aarch64-apple-darwin | runs | supported |
| aarch64-unknown-linux-gnu | emits | supported |
| arm64-apple-ios | emits | supported |
| wasm32-wasip1 | runs (node:wasi, wasmtime): docs/design/wasm.md | portability target: no guaranteed FMA, tail calls by the wasm tail-call extension |
| wasm32-unknown-unknown | emits; runtime-os Linux is a stand-in | portability target: no guaranteed FMA, tail calls by the wasm tail-call extension |
| riscv64-unknown-linux-gnu | emits | parked: LLVM has no tailcc for RISC-V |
| x86_64-unknown-linux-musl | runs; fully static | supported |
| aarch64-unknown-linux-musl | emits and runs (qemu-user); fully static | supported |
| nvptx64-nvidia-cuda | emits PTX (examples/gpu); runtime-os Linux is a stand-in, no runtime is emitted | kernel target: emits PTX for a driver to launch (--emit ptx); no host runtime, no tail calls, FMA yes (docs/design/gpu.md) |
| wgsl-unknown-webgpu | emits WGSL (examples/webgpu); runtime-os Linux is a stand-in, no runtime is emitted | kernel target: emits WGSL for a WebGPU driver to launch (--emit wgsl); no host runtime, no tail calls, FMA yes (docs/design/webgpu.md) |
