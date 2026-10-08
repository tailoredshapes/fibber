#!/usr/bin/env python3
"""compiler/tests/native/gpu/cuda_run.py: runs the PTX of fib.gpu's atomics and reductions on a CUDA device through libcuda (ctypes) and compares with the CPU.
usage: cuda_run.py reduce REDUCE.ptx REF.txt      the grid reductions against fibber's CPU answers (REF.txt: what `fibc run examples/gpu/reduce.fib` printed)
       cuda_run.py atomics ATOMICS.ptx            the atomics kernels of examples/gpu/atomics.fib against Python's count
       cuda_run.py badgrid REDUCE.ptx             a wrong `blocks` argument is a device trap, reported by the sync
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
            check(abs(got - cpu) <= bound, f"sum-f32 n={n} grid={blocks} block={block}: device {got!r}, CPU {cpu!r}, |diff| {abs(got - cpu):.3g} within the bound {bound:.3g}")


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


mode = sys.argv[1]
if mode == "reduce":
    reduce_mode(sys.argv[2], sys.argv[3])
elif mode == "atomics":
    atomics_mode(sys.argv[2])
else:
    badgrid_mode(sys.argv[2])
sys.exit(1 if bad else 0)
