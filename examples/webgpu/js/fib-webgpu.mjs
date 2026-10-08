// examples/webgpu/js/fib-webgpu.mjs: the driver side of fibber's `Device` protocol in JavaScript (docs/design/webgpu.md 5), over WebGPU's
// `navigator.gpu` (a browser) or Dawn's (the `webgpu` npm package under node). `FibGpu` is the driver as a class; `wasmImports` is the same
// as the imports `env.fib_gpu_*` a fibber program built for wasm32-wasi calls (examples/webgpu/js/webgpu.fib declares them). WebGPU is
// asynchronous (requestAdapter, mapAsync, the queue) and a wasm import is a plain call, so the asynchronous imports are wrapped in
// `WebAssembly.Suspending` and the export is called through `WebAssembly.promising` (JavaScript Promise Integration: node 24+, Chrome 137+).
//
// The binding model is the WGSL's (compiler/native/wgsl.fib): group 0, binding 0 the uniform of scalar arguments (4 bytes each in order),
// binding 1 the trap flag, bindings 2.. the buffer arguments in order; the workgroup size is the override constants wg_x, wg_y, wg_z. A
// kernel's `// fib.kernel-sig NAME: T..` line (GPU-2's launch ABI) is the signature a launch is checked against. Every error reaches the module as a negative status
// and a text (`fib_gpu_error`); nothing throws into the module: no `navigator.gpu` is an Err of open.

export class GpuError extends Error {
  constructor(code, message) { super(message); this.code = code; }
}

const LIMITS = { maxBuffers: 7, maxScalars: 16 };

function parseSignatures(wgsl) {
  const kernels = new Map();
  for (const line of wgsl.split("\n")) {
    const m = /^\/\/ fib\.kernel-sig (\S+):((?: \S+)*)$/.exec(line);
    if (m) kernels.set(m[1], m[2].trim().split(/\s+/).filter(Boolean));
  }
  return kernels;
}

export class FibGpu {
  constructor(gpu) {
    this.gpu = gpu; this.device = null; this.adapterInfo = null;
    this.modules = []; this.kernels = []; this.buffers = []; this.pipelines = new Map();
    this.lastError = ""; this.pending = []; this.flag = null; this.flagRead = null; this.submitted = 0;
  }

  // The adapter and device, or a GpuError (no WebGPU in this host, no adapter, no device).
  async open() {
    if (!this.gpu) throw new GpuError(-2, "this host has no WebGPU (navigator.gpu is undefined)");
    const adapter = await this.gpu.requestAdapter();
    if (!adapter) throw new GpuError(-3, "no WebGPU adapter (no GPU, or WebGPU is disabled)");
    this.adapterInfo = adapter.info ?? {};
    this.device = await adapter.requestDevice();
    if (!this.device) throw new GpuError(-4, "no WebGPU device");
    this.device.addEventListener?.("uncapturederror", (e) => { this.lastError = String(e.error?.message ?? e.error); });
    this.flag = this.device.createBuffer({ size: 16, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST });
    this.flagRead = this.device.createBuffer({ size: 16, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
    // Bound to the buffer slots a kernel does not take: one each (binding one buffer twice is writable aliasing, which WebGPU refuses).
    this.dummies = Array.from({ length: LIMITS.maxBuffers }, () => this.device.createBuffer({ size: 16, usage: GPUBufferUsage.STORAGE }));
    return this.name();
  }

  name() {
    const i = this.adapterInfo ?? {};
    return [i.vendor, i.architecture, i.device, i.description].filter(Boolean).join(" ") || "WebGPU device";
  }

  close() {
    for (const b of this.buffers) if (b && b.state !== 2) { b.buf.destroy(); b.state = 2; }
    this.flag?.destroy(); this.flagRead?.destroy(); for (const d of this.dummies ?? []) d.destroy();
    this.device?.destroy?.(); this.device = null;
  }

  // A module of WGSL text: compiled by the host (Tint under Dawn, naga under wgpu); a compile error is a GpuError with the message.
  async loadWgsl(text) {
    const dev = this.need();
    dev.pushErrorScope("validation");
    const mod = dev.createShaderModule({ code: text });
    const info = await mod.getCompilationInfo();
    const err = await dev.popErrorScope();
    const errors = info.messages.filter((m) => m.type === "error").map((m) => `${m.lineNum}:${m.linePos}: ${m.message}`);
    if (err || errors.length) throw new GpuError(-10, "the WGSL does not compile: " + (errors.join("; ") || err.message));
    // The layout is explicit: `auto` would hold only the bindings an entry point uses, and a kernel that never traps does not read the flag.
    const nbufs = (text.match(/var<storage, read_write> buf\d+:/g) ?? []).length;
    const entry = (binding, type) => ({ binding, visibility: GPUShaderStage.COMPUTE, buffer: { type } });
    const entries = [entry(0, "uniform"), entry(1, "storage")];
    for (let i = 0; i < nbufs; i++) entries.push(entry(2 + i, "storage"));
    const bgl = dev.createBindGroupLayout({ entries });
    const layout = dev.createPipelineLayout({ bindGroupLayouts: [bgl] });
    this.modules.push({ mod, sigs: parseSignatures(text), nbufs, bgl, layout });
    return this.modules.length - 1;
  }

  kernel(modId, name) {
    const m = this.modules[modId];
    if (!m) throw new GpuError(-11, `no module ${modId}`);
    const sig = m.sigs.get(name);
    if (!sig) throw new GpuError(-12, `no kernel ${name} in the module (its kernels: ${[...m.sigs.keys()].join(", ") || "none"})`);
    this.kernels.push({ mod: m.mod, name, sig, module: m });
    return this.kernels.length - 1;
  }

  buffer(bytes) {
    const dev = this.need();
    if (bytes <= 0) throw new GpuError(-1, `a buffer of ${bytes} bytes`);
    const size = (bytes + 3) & ~3;
    const buf = dev.createBuffer({ size, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST });
    this.buffers.push({ buf, bytes: size, state: 0 });
    return this.buffers.length - 1;
  }

  bufferOf(id, call) {
    const b = this.buffers[id];
    if (!b) throw new GpuError(-1, `${call}: no buffer ${id}`);
    if (b.state === 2) throw new GpuError(-1, `${call}: the buffer was freed`);
    if (b.state === 1) throw new GpuError(-1, `${call}: the buffer is lent to a running kernel: sync first`);
    return b;
  }

  upload(id, bytes) {
    const b = this.bufferOf(id, "upload");
    if (bytes.byteLength > b.bytes) throw new GpuError(-1, `upload: ${bytes.byteLength} bytes do not fit a buffer of ${b.bytes}`);
    this.need().queue.writeBuffer(b.buf, 0, bytes);
  }

  // The first `n` bytes of the buffer, after the queue's work so far (a staging copy, mapped when the copy is done).
  async download(id, n) {
    const b = this.bufferOf(id, "download");
    if (n > b.bytes) throw new GpuError(-1, `download: ${n} bytes do not fit a buffer of ${b.bytes}`);
    const dev = this.need();
    const size = (n + 3) & ~3;
    const staging = dev.createBuffer({ size, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
    const enc = dev.createCommandEncoder();
    enc.copyBufferToBuffer(b.buf, 0, staging, 0, size);
    dev.queue.submit([enc.finish()]);
    await staging.mapAsync(GPUMapMode.READ);
    const out = new Uint8Array(staging.getMappedRange().slice(0, n));
    staging.unmap(); staging.destroy();
    return out;
  }

  release(id) {
    const b = this.bufferOf(id, "release");
    b.buf.destroy(); b.state = 2;
  }

  pipeline(k, block) {
    const key = `${this.kernels.indexOf(k)}/${block.join("x")}`;
    let p = this.pipelines.get(key);
    if (!p) {
      p = this.need().createComputePipeline({
        layout: k.module.layout,
        compute: { module: k.mod, entryPoint: k.name, constants: { wg_x: block[0], wg_y: block[1], wg_z: block[2] } },
      });
      this.pipelines.set(key, p);
    }
    return p;
  }

  // A launch: `args` are {kind: 0 buffer | 1 i32 | 2 f32, value}, checked against the kernel's signature; the buffers are lent until sync.
  launch(kid, grid, block, args) {
    const k = this.kernels[kid];
    if (!k) throw new GpuError(-1, `launch: no kernel ${kid}`);
    const dev = this.need();
    if (args.length !== k.sig.length) throw new GpuError(-1, `launch ${k.name}: ${args.length} arguments for ${k.sig.length} parameters (${k.sig.join(" ")})`);
    const entries = [];
    const scalars = new Uint32Array(16);
    let nb = 0, ns = 0;
    const lent = [];
    args.forEach((a, i) => {
      const want = k.sig[i];
      if (a.kind === 0) {
        if (want !== "ptr") throw new GpuError(-1, `launch ${k.name}: argument ${i} is a buffer, the kernel's parameter ${i} is ${want}`);
        const b = this.bufferOf(a.value, `launch ${k.name}`);
        if (lent.includes(b)) throw new GpuError(-1, `launch ${k.name}: one buffer as two arguments (a kernel that writes in place takes one pointer)`);
        entries.push({ binding: 2 + nb, resource: { buffer: b.buf } }); nb++; lent.push(b);
      } else {
        if (want === "ptr") throw new GpuError(-1, `launch ${k.name}: argument ${i} is a scalar, the kernel's parameter ${i} is a buffer`);
        if (ns >= LIMITS.maxScalars) throw new GpuError(-1, `launch ${k.name}: more than ${LIMITS.maxScalars} scalar arguments`);
        scalars[ns++] = a.kind === 2 ? new Uint32Array(new Float32Array([a.value]).buffer)[0] : (a.value >>> 0);
      }
    });
    const uniform = dev.createBuffer({ size: 64, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    dev.queue.writeBuffer(uniform, 0, scalars);
    dev.queue.writeBuffer(this.flag, 0, new Uint32Array(4));
    entries.push({ binding: 0, resource: { buffer: uniform } }, { binding: 1, resource: { buffer: this.flag } });
    for (let i = nb; i < k.module.nbufs; i++) entries.push({ binding: 2 + i, resource: { buffer: this.dummies[i] } });
    const pipe = this.pipeline(k, block);
    const bg = dev.createBindGroup({ layout: k.module.bgl, entries });
    const enc = dev.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipe); pass.setBindGroup(0, bg); pass.dispatchWorkgroups(grid[0], grid[1], grid[2]); pass.end();
    enc.copyBufferToBuffer(this.flag, 0, this.flagRead, 0, 16);
    dev.queue.submit([enc.finish()]);
    this.submitted++;
    for (const b of lent) b.state = 1;
    this.pending.push({ uniform, lent });
  }

  // Waits for the queue; the lent buffers are the caller's again; a set trap flag is the GpuError here.
  async sync() {
    const dev = this.need();
    await dev.queue.onSubmittedWorkDone();
    for (const p of this.pending) { for (const b of p.lent) if (b.state === 1) b.state = 0; p.uniform.destroy(); }
    const launched = this.pending.length;
    this.pending = [];
    if (launched === 0) return;
    await this.flagRead.mapAsync(GPUMapMode.READ);
    const flag = new Uint32Array(this.flagRead.getMappedRange())[0];
    this.flagRead.unmap();
    if (flag !== 0) throw new GpuError(-20, "a kernel trapped (the device assert: a `trap`, a checked + that overflowed)");
    if (this.lastError) { const e = this.lastError; this.lastError = ""; throw new GpuError(-21, "WebGPU reported: " + e); }
  }

  need() {
    if (!this.device) throw new GpuError(-5, "no device: open first");
    return this.device;
  }
}

// The imports `env.fib_gpu_*` for a fibber wasm32 module (examples/webgpu/js/webgpu.fib): `memory()` gives the instance's memory (the
// instance exists after the imports). Every import answers a status (0 or an id; negative an error) and keeps the error's text for
// `fib_gpu_error`; the asynchronous ones suspend the module (JSPI).
export function wasmImports(fib, memory) {
  let lastError = "";
  const bytes = (p, n) => new Uint8Array(memory().buffer, p, n);
  const text = (p, n) => new TextDecoder().decode(bytes(p, n));
  const guard = (f) => (...a) => { try { const r = f(...a); return r === undefined ? 0 : r; } catch (e) { lastError = e.message ?? String(e); return e instanceof GpuError ? e.code : -99; } };
  const guardAsync = (f) => async (...a) => { try { const r = await f(...a); return r === undefined ? 0 : r; } catch (e) { lastError = e.message ?? String(e); return e instanceof GpuError ? e.code : -99; } };
  const put = (p, cap, s) => { const b = new TextEncoder().encode(s); const n = Math.min(b.length, cap - 1); bytes(p, n).set(b.subarray(0, n)); bytes(p + n, 1)[0] = 0; return n; };
  const S = (f) => new WebAssembly.Suspending(f);
  return {
    fib_gpu_open: S(guardAsync(async () => { await fib.open(); return 0; })),
    fib_gpu_close: guard(() => { fib.close(); return 0; }),
    fib_gpu_name: guard((p, cap) => put(p, cap, fib.name())),
    fib_gpu_error: (p, cap) => put(p, cap, lastError),
    fib_gpu_load: S(guardAsync((p, n) => fib.loadWgsl(text(p, n)))),
    fib_gpu_kernel: guard((m, p, n) => fib.kernel(m, text(p, n))),
    fib_gpu_buffer: guard((n) => fib.buffer(n)),
    fib_gpu_upload: guard((b, p, n) => { fib.upload(b, bytes(p, n).slice()); return 0; }),
    fib_gpu_download: S(guardAsync(async (b, p, n) => { const out = await fib.download(b, n); bytes(p, n).set(out); return 0; })),
    fib_gpu_launch: guard((k, gx, gy, gz, bx, by, bz, p, n) => {
      const t = new DataView(memory().buffer, p, 8 * n);
      const args = [];
      for (let i = 0; i < n; i++) {
        const kind = t.getInt32(8 * i, true);
        args.push({ kind, value: kind === 2 ? t.getFloat32(8 * i + 4, true) : t.getInt32(8 * i + 4, true) });
      }
      fib.launch(k, [gx, gy, gz], [bx, by, bz], args); return 0;
    }),
    fib_gpu_sync: S(guardAsync(async () => { await fib.sync(); return 0; })),
    fib_gpu_release: guard((b) => { fib.release(b); return 0; }),
  };
}
