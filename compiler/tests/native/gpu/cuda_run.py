#!/usr/bin/env python3
"""compiler/tests/native/gpu/cuda_run.py: runs the PTX of fib.gpu's atomics and reductions on a CUDA device through libcuda (ctypes) and compares with the CPU.
usage: cuda_run.py reduce REDUCE.ptx REF.txt      the grid reductions against fibber's CPU answers (REF.txt: what `fibc run examples/gpu/reduce.fib` printed)
       cuda_run.py atomics ATOMICS.ptx            the atomics kernels of examples/gpu/atomics.fib against Python's count
       cuda_run.py badgrid REDUCE.ptx             a wrong `blocks` argument is a device trap, reported by the sync
       cuda_run.py warp WARP.ptx REF.txt          GPU-5: the warp-shuffle reductions against the same reference, a block below the warp is a trap
       cuda_run.py probe WARP.ptx                 GPU-5: shfl-down/up/xor/idx, lane-id, subgroup-size and ballot against a model of the warp
       cuda_run.py f32atomics F32.ptx             GPU-5: atom.global.add.f32 and the f32 max/min against the CPU
A test harness only (the product's driver is fib-gpu-cuda). It loads the PTX with cuModuleLoadData (the driver compiles it, so a PTX the driver rejects fails
here), launches by kernel name, and prints one line per check: `ok ...` or `FAIL ...`. Exit 0 when every check holds, 1 on a failure, 3 when there is no
CUDA device (the caller skips)."""
import ctypes as C, struct, sys

try:
    cu = C.CDLL("libcuda.so.1")
    if cu.cuInit(0) != 0:
        raise OSError("cuInit")
except OSError:
    sys.exit(3)


def ck(r, what):
    if r != 0:
        s = C.c_char_p()
        cu.cuGetErrorString(r, C.byref(s))
        raise RuntimeError(f"{what}: error {r} {s.value}")


class Dev:
    def __init__(self, ptx_path):
        d = C.c_int()
        ck(cu.cuDeviceGet(C.byref(d), 0), "device")
        self.ctx = C.c_void_p()
        ck(cu.cuCtxCreate_v2(C.byref(self.ctx), 0, d), "context")
        self.mod = C.c_void_p()
        self.src = C.create_string_buffer(open(ptx_path, "rb").read() + b"\0")
        ck(cu.cuModuleLoadData(C.byref(self.mod), self.src), "module load of " + ptx_path)

    def alloc(self, data):
        p = C.c_uint64()
        ck(cu.cuMemAlloc_v2(C.byref(p), max(len(data), 4)), "alloc")
        ck(cu.cuMemcpyHtoD_v2(p, data, len(data)), "upload")
        return p.value

    def read(self, p, n):
        b = C.create_string_buffer(n)
        ck(cu.cuMemcpyDtoH_v2(b, C.c_uint64(p), n), "download")
        return b.raw

    def launch(self, name, grid, block, *args):
        """args: ('p', device pointer) or ('i', i32)"""
        f = C.c_void_p()
        ck(cu.cuModuleGetFunction(C.byref(f), self.mod, name.encode()), "kernel " + name)
        cargs = [C.c_int32(v) if k == "i" else C.c_uint64(v) for k, v in args]
        arr = (C.c_void_p * len(cargs))(*[C.cast(C.byref(c), C.c_void_p) for c in cargs])
        ck(cu.cuLaunchKernel(f, grid, 1, 1, block, 1, 1, 0, None, arr, None), "launch " + name)
        ck(cu.cuCtxSynchronize(), "sync " + name)


bad = 0


def check(ok, text):
    global bad
    print(("ok   " if ok else "FAIL ") + text)
    bad += 0 if ok else 1


def lcg(n, seed):  # fib.gpu.reduce-ref: s(i+1) = (s(i) * 1664525 + 1013904223) mod 2^32
    out, s = [], seed
    for _ in range(n):
        s = (s * 1664525 + 1013904223) % 2**32
        out.append(s)
    return out


def bits(x):
    return struct.unpack("i", struct.pack("f", x))[0]


def P(v): return ("p", v)
def I(v): return ("i", v)


def reduce_mode(ptx, ref):
    refs = {}
    for line in open(ref):
        w = line.split()
        if w and w[0] == "ref":
            refs[(w[1], int(w[2]), int(w[3]), int(w[4]))] = w[5:]
    red = Dev(ptx)
    for (name, n, blocks, block), want in sorted(refs.items(), key=lambda kv: (-kv[0][1], kv[0][2], kv[0][0])):
        if name == "sum-f32":
            continue
        s = lcg(n, n)
        xi = [v - 2**31 for v in s]
        xf = [(v >> 8) / 2**23 - 1.0 for v in s]
        pi = red.alloc(struct.pack(f"{n}i", *xi))
        pf = red.alloc(struct.pack(f"{n}f", *xf))
        tag = f"{name} n={n} grid={blocks} block={block}"
        if name in ("sum-i32", "max-i32", "min-i32"):
            ident = {"sum-i32": 0, "max-i32": -2**31, "min-i32": 2**31 - 1}[name]
            po = red.alloc(struct.pack("i", ident))
            red.launch("reduce_" + name.replace("-", "_"), blocks, block, P(pi), P(po), I(n), I(blocks))
            got = struct.unpack("i", red.read(po, 4))[0]
            check(got == int(want[0]), f"{tag}: device {got}, fibber's CPU {want[0]}")
        elif name == "max-f32":
            po = red.alloc(struct.pack("f", -3.4028235e38))
            red.launch("reduce_max_f32", blocks, block, P(pf), P(po), I(n), I(blocks))
            got = bits(struct.unpack("f", red.read(po, 4))[0])
            check(got == int(want[0]), f"{tag}: device bits {got}, CPU bits {want[0]}")
        elif name == "partials":
            pp = red.alloc(bytes(4 * blocks))
            red.launch("reduce_partials_f32", blocks, block, P(pf), P(pp), I(n), I(blocks))
            got = [bits(v) for v in struct.unpack(f"{blocks}f", red.read(pp, 4 * blocks))]
            exp = [int(v) for v in want[0].split(",")]
            check(got == exp, f"{tag}: {blocks} partials bit for bit with the CPU's tree order")
            sref = refs[("sum-f32", n, blocks, block)]
            po = red.alloc(struct.pack("f", 0.0))
            red.launch("reduce_sum_f32", blocks, block, P(pf), P(po), I(n), I(blocks))
            got = struct.unpack("f", red.read(po, 4))[0]
            cpu = struct.unpack("f", struct.pack("i", int(sref[0])))[0]
            bound = float(sref[1])
            tight = min(bound, 0.1)  # the documented bound is loose for this data (3e4 against a sum of 276); the measured differences are about 5e-3
            check(abs(got - cpu) <= tight, f"sum-f32 n={n} grid={blocks} block={block}: device {got!r}, CPU {cpu!r}, |diff| {abs(got - cpu):.3g} within {tight:.3g} (documented bound {bound:.3g})")


def atomics_mode(ptx):
    d = Dev(ptx)
    n = 100000
    s = lcg(n, 5)
    grid = (n + 255) // 256
    # hist: 16 bins
    data = [(v >> 12) & 15 for v in s]
    pd, pb = d.alloc(struct.pack(f"{n}i", *data)), d.alloc(bytes(64))
    d.launch("hist", grid, 256, P(pd), P(pb), I(n))
    got = list(struct.unpack("16i", d.read(pb, 64)))
    check(got == [data.count(k) for k in range(16)], f"hist n={n}: 16 bins equal the CPU's count ({got[:3]}..)")
    # tally: count above, signed max/min, unsigned max
    xs = [v - 2**31 for v in s]
    limit = 1000000
    po = d.alloc(struct.pack("4i", 0, -2**31, 2**31 - 1, 0))
    pd = d.alloc(struct.pack(f"{n}i", *xs))
    d.launch("tally", grid, 256, P(pd), P(po), I(n), I(limit))
    got = struct.unpack("4i", d.read(po, 16))
    want = (sum(1 for x in xs if x > limit), max(xs), min(xs), struct.unpack("i", struct.pack("I", max(x % 2**32 for x in xs)))[0])
    check(got == want, f"tally n={n}: add, max, min, umax {got} equal the CPU's {want}")
    # claim: 8 slots, one winner each
    for m in (n, 5):
        ps, po = d.alloc(bytes(32)), d.alloc(bytes(4))
        d.launch("claim", (m + 255) // 256, 256, P(ps), P(po), I(m))
        won = struct.unpack("i", d.read(po, 4))[0]
        slots = struct.unpack("8i", d.read(ps, 32))
        mine = all(v == 0 or (v - 1) % 8 == k for k, v in enumerate(slots))
        check(won == min(m, 8) and mine and sum(1 for v in slots if v) == min(m, 8), f"claim n={m}: compare-and-swap gave {won} winners, one per slot, each slot holds a thread of its own residue")
    # block-max through a shared word
    blocks = (n + 127) // 128
    po = d.alloc(struct.pack(f"{blocks}i", *([-2**31] * blocks)))
    d.launch("block_max", blocks, 128, P(pd), P(po), I(n))
    got = list(struct.unpack(f"{blocks}i", d.read(po, 4 * blocks)))
    want = [max(xs[b * 128:(b + 1) * 128]) for b in range(blocks)]
    check(got == want, f"block-max n={n}: {blocks} block maxima through shared-memory atomics equal the CPU's")


def badgrid_mode(ptx):
    red = Dev(ptx)
    xs, po = red.alloc(struct.pack("4i", 1, 2, 3, 4)), red.alloc(struct.pack("i", 0))
    try:
        red.launch("reduce_sum_i32", 4, 1, P(xs), P(po), I(4), I(3))
        check(False, "a wrong blocks argument was not caught")
    except RuntimeError as e:
        check("sync" in str(e), f"a wrong blocks argument (3 for a grid of 4) is a device trap, reported by the sync: {e}")


def warp_mode(ptx, ref):
    """GPU-5: the grid reductions by warp shuffle against the same reference lines (the partials' sum within the documented bound, tightly checked too), then a block smaller
    than the warp, which must end in a device trap (last: a trap poisons the context)."""
    refs = {}
    for line in open(ref):
        w = line.split()
        if w and w[0] == "ref":
            refs[(w[1], int(w[2]), int(w[3]), int(w[4]))] = w[5:]
    red = Dev(ptx)
    for (name, n, blocks, block), want in sorted(refs.items(), key=lambda kv: (-kv[0][1], kv[0][2], kv[0][0])):
        if name == "sum-f32" or block < 32:
            continue
        s = lcg(n, n)
        xi = [v - 2**31 for v in s]
        xf = [(v >> 8) / 2**23 - 1.0 for v in s]
        pi = red.alloc(struct.pack(f"{n}i", *xi))
        pf = red.alloc(struct.pack(f"{n}f", *xf))
        tag = f"warp {name} n={n} grid={blocks} block={block}"
        if name in ("sum-i32", "max-i32", "min-i32"):
            ident = {"sum-i32": 0, "max-i32": -2**31, "min-i32": 2**31 - 1}[name]
            po = red.alloc(struct.pack("i", ident))
            red.launch("reduce_warp_" + name.replace("-", "_"), blocks, block, P(pi), P(po), I(n), I(blocks))
            got = struct.unpack("i", red.read(po, 4))[0]
            check(got == int(want[0]), f"{tag}: device {got}, fibber's CPU {want[0]}")
        elif name == "max-f32":
            po = red.alloc(struct.pack("f", -3.4028235e38))
            red.launch("reduce_warp_max_f32", blocks, block, P(pf), P(po), I(n), I(blocks))
            got = bits(struct.unpack("f", red.read(po, 4))[0])
            check(got == int(want[0]), f"{tag}: device bits {got}, CPU bits {want[0]}")
        elif name == "partials":
            pp = red.alloc(bytes(4 * blocks))
            red.launch("reduce_warp_partials_f32", blocks, block, P(pf), P(pp), I(n), I(blocks))
            got = sum(struct.unpack(f"{blocks}f", red.read(pp, 4 * blocks)))
            sref = refs[("sum-f32", n, blocks, block)]
            cpu = struct.unpack("f", struct.pack("i", int(sref[0])))[0]
            bound = float(sref[1])
            tight = min(bound, 0.1)  # measured differences are about 5e-3: a kernel that wrote zeros is within the documented bound of nothing worth having
            check(abs(got - cpu) <= tight, f"{tag}: sum of the {blocks} partials {got!r}, CPU {cpu!r}, |diff| {abs(got - cpu):.3g} within {tight:.3g} (documented bound {bound:.3g})")
    xs, po = red.alloc(struct.pack("32i", *([1] * 32))), red.alloc(struct.pack("i", 0))
    try:
        red.launch("reduce_warp_sum_i32", 2, 16, P(xs), P(po), I(32), I(2))
        check(False, "warp: a block of 16 threads, less than the warp, was not caught")
    except RuntimeError as e:
        check("sync" in str(e), f"warp: a block of 16 threads, less than the warp, is a device trap: {e}")


def probe_mode(ptx):
    """GPU-5: the primitives against a model of the warp (32 lanes). PTX answers a lane's own value for a source out of range, which is checked too."""
    d = Dev(ptx)
    blocks, block = 4, 64
    threads = blocks * block
    po = d.alloc(bytes(4 * 8 * threads))
    d.launch("warp_probe", blocks, block, P(po), I(threads))
    g = struct.unpack(f"{8 * threads}i", d.read(po, 4 * 8 * threads))
    S = g[1]
    v = lambda t: 7 * t + 3
    wrong, first = 0, ""
    bal = sum(1 << l for l in range(min(S, 32)) if l % 3 == 0)
    bal = struct.unpack("i", struct.pack("I", bal))[0]
    for t in range(threads):
        base, lane, o = t - t % S, t % S, t * 8
        want = [lane, S, v(t + 1) if lane + 1 < S else v(t), v(t - 2) if lane >= 2 else v(t),
                v(base + (lane ^ 5)) if (lane ^ 5) < S else v(t), v(base + 3), bal,
                bits(float(v(t + 2) if lane + 2 < S else v(t)))]
        for k in range(8):
            if g[o + k] != want[k]:
                wrong += 1
                first = first or f"thread {t} word {k}: {g[o + k]}, model {want[k]}"
    check(wrong == 0, f"probe: {threads} threads, warp of {S}: lane, size, shuffles down/up/xor/idx (i32 and f32; out of range answers the lane's own) and the ballot equal the model" + (f"; first: {first}" if first else ""))


def f32atomics_mode(ptx):
    """GPU-5: the native f32 atomic add (atom.global.add.f32) and the max/min loops, against the CPU."""
    d = Dev(ptx)
    n = 100000
    s = lcg(n, 9)
    xf = [(v >> 8) / 2**23 - 1.0 for v in s]
    xf = list(struct.unpack(f"{n}f", struct.pack(f"{n}f", *xf)))
    grid = (n + 255) // 256
    pd = d.alloc(struct.pack(f"{n}f", *xf))
    po = d.alloc(struct.pack("f", 0.0))
    d.launch("f32_sum", grid, 256, P(pd), P(po), I(n))
    got = struct.unpack("f", d.read(po, 4))[0]
    cpu = 0.0
    for x in xf:
        cpu = struct.unpack("f", struct.pack("f", cpu + x))[0]
    bound = n * 2**-24 * sum(abs(x) for x in xf)
    tight = min(bound, 0.05)  # the documented bound is 297 for this data (a sum of 16); the measured differences are about 3e-4
    check(abs(got - cpu) <= tight, f"f32 atomic add n={n}: device {got!r}, CPU {cpu!r}, |diff| {abs(got - cpu):.3g} within {tight:.3g} (documented bound {bound:.3g})")
    po = d.alloc(struct.pack("2f", -3.4028235e38, 3.4028235e38))
    d.launch("f32_minmax", grid, 256, P(pd), P(po), I(n))
    mm = struct.unpack("2f", d.read(po, 8))
    check(mm == (max(xf), min(xf)), f"f32 atomic max/min n={n}: device {mm} equal the CPU's {(max(xf), min(xf))} exactly")


mode = sys.argv[1]
if mode == "reduce":
    reduce_mode(sys.argv[2], sys.argv[3])
elif mode == "atomics":
    atomics_mode(sys.argv[2])
elif mode == "warp":
    warp_mode(sys.argv[2], sys.argv[3])
elif mode == "probe":
    probe_mode(sys.argv[2])
elif mode == "f32atomics":
    f32atomics_mode(sys.argv[2])
else:
    badgrid_mode(sys.argv[2])
sys.exit(1 if bad else 0)
