# Examples

Run these from the repository root with the current compiler and `-I lib`.
`make doc-examples` uses [smoke.json](smoke.json) to run or compile the entries
below. Runtime checks require a zero main result; compilation checks do not
claim network, driver, static-link or hardware validation.

| Source | Try | Documentation smoke check |
|---|---|---|
| [tensor.fib](tensor.fib) | `fibc run examples/tensor.fib` | run |
| [logic.fib](logic.fib) | `fibc run examples/logic.fib` | run |
| [exclusive-views.fib](exclusive-views.fib) | `fibc run -O 2 examples/exclusive-views.fib` | run; timings are illustrative |
| [sudoku.fib](sudoku.fib) | `fibc run examples/sudoku.fib -I examples -- examples/sudoku/test.txt` | run with test puzzle |
| [gpu/gpu.fib](gpu/gpu.fib) | `fibc run examples/gpu/gpu.fib -I examples/gpu` | host kernel comparisons |
| [gpu/reduce.fib](gpu/reduce.fib) | `fibc run examples/gpu/reduce.fib` | host reduction comparisons |
| [webgpu/cpu.fib](webgpu/cpu.fib) | `fibc run examples/webgpu/cpu.fib -I examples/webgpu` | CPU reference |
| [http-server.fib](http-server.fib) | `fibc build examples/http-server.fib -o server` | compile; starts a server when run |
| [http-client.fib](http-client.fib) | `fibc build examples/http-client.fib -o client` | compile; requests need a server |
| [lz4.fib](lz4.fib) | `fibc build examples/lz4.fib -o lz4f` | compile; file round trips are in library specs |
| [static-demo/demo.fib](static-demo/demo.fib) | `fibc build --static examples/static-demo/demo.fib -o demo` | native compile; static check needs musl |
| [static-demo/fetch.fib](static-demo/fetch.fib) | `fibc build --static examples/static-demo/fetch.fib -o fetch` | native compile; DNS/network need environment |
| [static-demo/bootstrap.fib](static-demo/bootstrap.fib) | `fibc build --static examples/static-demo/bootstrap.fib -o bootstrap` | native compile; runtime needs Runtime API |

[gpu/kernels.fib](gpu/kernels.fib), [gpu/atomics.fib](gpu/atomics.fib) and
[webgpu/kernels.fib](webgpu/kernels.fib) are modules for the host/device tests,
not standalone application entry points. External CUDA/WebGPU drivers and
hardware are needed for [webgpu/host-cuda.fib](webgpu/host-cuda.fib) and
[webgpu/host.fib](webgpu/host.fib); the docs smoke check does not claim those runs.
[wasm/kernels.fib](wasm/kernels.fib) and its [JavaScript host](wasm/kernels.mjs)
need a WASI sysroot/linker/runtime. `make wasm` checks that optional toolchain.

See [GPU guidance](../docs/guide/gpu-kernels.md),
[deployment](../docs/guide/targets-and-deployment.md) and each file's command
comments for setup beyond the core smoke checks.
