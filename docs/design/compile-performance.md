# Compile performance

Where a cold self-build spends its time and memory, and whether per-unit parallel LLVM can serve release and bootstrap builds.
Measured 2026-10-12 on the lilnas box (i7-14700KF, 28 threads, 61 GiB, Linux 7.0, LLVM 21.1.8, shared with other agents: load average 3 to 11, up to 33 for a few runs, noted per row). Every heavy job ran under `nice -n 10`. Code under test: `main` at cc5ba368 plus the opt-in prototype of this branch. These are observations, not CI durations.

## Summary

| | whole program (today's release/bootstrap) | 16 units, parallel (prototype) | 16 units + copies of small callees (`FIB_LIRO_IMPORT=80`) |
|---|---:|---:|---:|
| cold `fibc build compiler/fibc.fib` wall, min / median of 3 | 101.0 / 101.2 s | 34.7 / 35.3 s | 48.8 / 49.8 s |
| peak RSS | 2075 MiB | 1359 MiB | 1731 MiB |
| executable | 11.1 MiB | 7.0 MiB | 15.5 MiB |
| speed of the resulting compiler (`F emit compiler/fibc.fib`, median of 3) | 23.5 s | 33.4 s (+42%) | 25.1 s (+7%) |
| user programs (`scripts/bench`, below) | baseline | 1.3x to 3.6x slower | 0.9x to 1.2x |
| byte-identical across 3 builds | yes | yes | yes |

1. The whole-program build is one thread (CPU/wall = 0.98) and LLVM is 77% of it: optimize 39.4 s, codegen 37.0 s. The front end and emitter are 22 s of serial fibber code before LLVM starts.
2. Compiling the 19 units in parallel (8 workers) takes the cold build from 101 s to 35 s. The floor is then the serial front end plus emitter (about 23 s) plus the biggest unit (the walkers group, 4.5 s) plus link.
3. Plain partitioning costs run time: the compiler is 30 to 40% slower and small user programs up to 3.6x slower, because `fib.*` runtime fast paths (retain/release/drop), prelude specialisations and drop walkers no longer inline into their callers. Copying small callees into each unit as internal definitions recovers most of it: `FIB_LIRO_IMPORT=80` leaves the compiler +7% and user programs within noise of whole-program, for 14 s more build wall and 1.3x the memory of plain units.
4. The fixed point holds with a partitioned-built F: F (16 units) builds F3 the same way, and the two executables are byte-identical (sha256 5c354c57...) and emit identical lIR (md5 aee45d6c...). The whole-program F and F3 are also byte-identical (sha256 967877e6...).

Recommendation at the end.

## Commands

```sh
# build F from this tree, then, with F:
python3 scripts/perf/run.py OUT.json -- env FIB_TIME=1 FIB_LIRO_CACHE=off build/F build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o /tmp/F2   # wall, user/sys, RSS every 0.25 s, timestamped phases
perf record -F 199 -g --call-graph fp -k CLOCK_MONOTONIC -- python3 scripts/perf/run.py ...   # needs kernel.perf_event_paranoid <= 1 (was 4; set to 1 with sudo for the run, put back to 4 after)
perf script -F comm,time,ip,sym,dso > s.txt;  python3 scripts/perf/analyze.py OUT.json s.txt        # per-phase top symbols
perf stat -e task-clock,cycles,instructions,cache-misses,context-switches,page-faults -- build/F build ...   # whole and partitioned
# the experiment: three builds then three emit timings per config (cp -r compiler lib to $FROZEN first so every config sees the same input)
FROZEN=/path/frozen BUILDER=build/F python3 scripts/perf/matrix.py \
  whole: h16:FIB_LIRO_PARTITION=1,FIB_LIRO_JOBS=8 i80:FIB_LIRO_PARTITION=1,FIB_LIRO_JOBS=8,FIB_LIRO_IMPORT=80 \
  ns3x32:FIB_LIRO_PARTITION=1,FIB_LIRO_JOBS=16,FIB_LIRO_UNITS=32,FIB_LIRO_SPLIT=ns3
python3 scripts/perf/progs.py num-f64,lazy-fused,strings      # small programs: build and run time per mode
BENCH_FIBC=F GATE_OUT=DIR FIB_LIRO_CACHE=DIR2 [FIB_LIRO_MODE=whole|FIB_LIRO_IMPORT=80] scripts/bench/quick.sh
```

## The knobs (all opt-in; defaults unchanged)

| variable | meaning |
|---|---|
| `FIB_LIRO_PARTITION=1` | a cache-off build partitions instead of compiling whole-program (today `FIB_LIRO_CACHE=off` forces whole-program) |
| `FIB_LIRO_JOBS=N` | compile up to N units at once (default 1); cache-off partitioned builds only; the objects are written and linked in unit order, so the executable does not depend on N |
| `FIB_LIRO_UNITS=N` | N code groups instead of 16 |
| `FIB_LIRO_SPLIT=hash\|ns1\|ns2\|ns3` | group by function-name hash (default) or by the first 1 to 3 dotted segments of the namespace |
| `FIB_LIRO_IMPORT=T`, `FIB_LIRO_IMPORT_DEPTH=D` | copy callees of at most T expression nodes into each unit that calls them as internal definitions (D rounds, default 2); the glue unit gets none |
| `FIB_LIRO_STATS=1` | also prints items and definitions per unit |

Code: `compiler/native/liro/parallel.fib` (workers: `pmap-range` over the units, each in its own LLVM context and target machine), `partition.fib` (split and copies), `build.fib`. Test: `parallel_checks` in `compiler/tests/native/liro/check.py` (default cache-off stays whole-program; jobs 1, 4, 4 give identical executables; splits and copies run correctly). `make quick`, `make adr` and `build/tools/sh-native-liro.ok` pass.

## Profile of the cold whole-program self-build

`FIB_TIME=1 FIB_LIRO_CACHE=off F build compiler/fibc.fib ...`, three runs on the matrix (quiet, load 3 to 6):

| phase | wall s (3 runs) | what it is |
|---|---:|---|
| front | 10.9, 11.5, 10.9 | read, expand, type, ownership check (fibber, single thread) |
| lir.emit | 10.7, 10.7, 11.0 | structured lIR, including macro-runner JIT |
| lir.check | 0.6 | |
| llvm.lower | 1.0 to 1.1 | |
| llvm.optimize | 39.4, 39.6, 40.9 | LLVM `default<O2>` over one 11 MiB module |
| llvm.codegen | 36.3, 37.0, 37.0 | SelectionDAG, regalloc, asm printer |
| total | 102.8, 101.1, 101.2 | |

CPU: user 99.6 s, sys 1.5 s over 101 s wall, so one thread end to end. `/usr/bin/time` agrees (user 102.0 + sys 1.6 for 103.7 s).
`perf stat` (whole): 99.95 s task-clock, 466 G P-core cycles, 846 G instructions (IPC 1.8), 1.1 M page faults, 1796 context switches, 923 migrations. Partitioned, 8 jobs: 48.4 s task-clock, 227 G cycles, 430 G instructions, 31.0 s elapsed. Whole-program needs 2.1x the instructions of the same program in 19 units; LLVM's passes are superlinear in module size.

Peak RSS by phase, sampled every 0.25 s from `/proc/PID/status` (profile run, box loaded 11 to 22, so the times are longer than above; sizes are not affected): front 0 to 841 MiB, lir.emit 845 to 1154, lir.check 1154, llvm.lower 1169 to 1316, llvm.optimize 1324 to 1915, llvm.codegen 1896 to 2048, link and teardown 1914 down to 1867. Peak 2048 MiB (2075 MiB by `wait4` on the quiet runs). Whole-program memory is the module plus its LLVM IR; the fibber side is about 1.1 GiB (`F emit` alone peaks at 1.3 GiB).

Top 20 leaf symbols per phase (`perf record -F 199`, 31,604 samples at 161 s wall under load, so absolute seconds are inflated; shares are what matter). `[unknown]` in the LLVM phases are symbols the distribution's libLLVM does not export; the frame-pointer call chains of fibber code are cut, so the useful inclusive view is in the notes after the lists.

**front: 0.0-13.0s, 2656 samples (~13.3 s CPU)**; F-whole 88%, [kernel] 6%, libc.so.6 5%, libLLVM.so.21.1 0%, [vdso] 0%

```text
10.5%  fib.drop-one
9.6%  fib.release-slow
5.9%  [kernel]
4.6%  [unknown]
4.5%  drop.14
3.7%  f.fib.prelude.mnode-get.i64.str
3.5%  f.fib.prelude.mnode-assoc.i64.str
2.5%  f.fib.prelude.vec-nth.str
2.4%  l.f.fib.prelude.vec-collect.str.1465
2.1%  f.fib.prelude.map-get.str.str
1.6%  f.fib.prelude.vec-conj.str
1.5%  f.fib.seq.types.lnode.str
1.5%  f.fib.prelude.array-insert.str
1.3%  fib.array-alloc
1.2%  f.fib.prelude.str-join
1.2%  f.fib.prelude.mnode-get.str.str
1.2%  f.types.table.table-find.str
1.1%  f.fib.prelude.array-each-from.str
0.9%  drop.8
0.9%  drop.183
```

**lir.emit: 13.0-27.1s, 2796 samples (~14.1 s CPU)**; F-whole 61%, libLLVM.so.21.1 29%, libc.so.6 7%, [kernel] 3%, ld-linux-x86-64.so.2 0%

```text
18.1%  [unknown]
6.4%  f.fib.prelude.vec-nth.str
3.4%  fib.release-slow
3.3%  fib.drop-one
2.9%  drop.14
2.5%  [kernel]
2.5%  f.fib.prelude.mnode-get.str.str
2.1%  f.native.jit.prune.visit
1.9%  f.native.lower.consts.lx-constant
1.8%  f.native.jit.prune.dead-internals
1.4%  f.fib.prelude.map-get.str.str
1.4%  f.fib.prelude.mnode-get.i64.str
1.3%  f.fib.prelude.vec-each-from.str
1.3%  fib.array-alloc
1.2%  f.fib.prelude.array-each-from.str
1.1%  f.fib.prelude.vec-conj.str
1.0%  f.native.jit.add-checked
0.9%  llvm::Value::getName() const
0.9%  f.fib.prelude.mnode-get.str.bool
0.9%  f.lir.check.check
```

**lir.check: 27.1-28.1s, 207 samples (~1.0 s CPU)**; F-whole 97%, libc.so.6 1%, [kernel] 1%, [vdso] 0%

```text
13.0%  f.fib.prelude.vec-nth.str
8.2%  drop.14
6.3%  fib.release-slow
5.8%  f.lir.check.walk.collect
5.3%  drop.8
5.3%  fib.drop-one
4.8%  f.fib.prelude.map-get.str.str
4.3%  f.fib.prelude.mnode-get.str.str
4.3%  f.fib.prelude.map-get.str.i64
3.4%  drop.590
2.9%  f.fib.prelude.mnode-assoc.str.str
2.4%  f.fib.prelude.array-insert.str
1.4%  drop.735
1.4%  [unknown]
1.4%  f.lir.check.expr.ty-inner
1.4%  f.lir.check.fcx.fcx-val
1.0%  drop.540
1.0%  [kernel]
1.0%  drop.1029
1.0%  drop.1190
```

**llvm.lower: 28.1-29.3s, 235 samples (~1.2 s CPU)**; F-whole 67%, libLLVM.so.21.1 23%, [kernel] 7%, libc.so.6 3%

```text
16.2%  [unknown]
13.2%  f.fib.prelude.vec-nth.str
9.8%  f.fib.prelude.map-get.str.i64
7.2%  [kernel]
5.5%  fib.drop-one
4.7%  f.native.lower.expr.lower-expr.owned
4.3%  fib.release-slow
3.0%  fib.array-alloc
3.0%  f.fib.prelude.mnode-get.str.str
1.3%  f.fib.prelude.map-assoc.str.str
1.3%  f.fib.prelude.vec-nth.i64
1.3%  llvm::StringMapImpl::LookupBucketFor(llvm::StringRef, unsigned int)
1.3%  f.native.lower.memory.lower-memory
1.3%  f.lir.check.walk.statements-into
1.3%  llvm::CallInst::init(llvm::FunctionType*, llvm::Value*, llvm::ArrayRef<llvm::Value*>, llvm
1.3%  f.fib.prelude.map-get.str.str
0.9%  drop.15
0.9%  f.native.lower.module-items
0.9%  f.native.lower.consts.lx-constant
0.9%  drop.3650
```

**llvm.optimize: 29.3-95.5s, 12823 samples (~64.4 s CPU)**; libLLVM.so.21.1 90%, libc.so.6 7%, [kernel] 2%, libstdc++.so.6.0.35 1%, ld-linux-x86-64.so.2 0%

```text
48.8%  [unknown]
2.0%  [kernel]
1.3%  llvm::SmallPtrSetImplBase::insert_imp_big(void const*)
1.1%  llvm::removeUnreachableBlocks(llvm::Function&, llvm::DomTreeUpdater*, llvm::MemorySSAUpdat
0.9%  malloc
0.7%  llvm::AnalysisManager<llvm::Function>::invalidate(llvm::Function&, llvm::PreservedAnalyses
0.6%  llvm::Instruction::getNumSuccessors() const
0.6%  llvm::Instruction::getSuccessor(unsigned int) const
0.6%  llvm::getUnderlyingObject(llvm::Value const*, unsigned int)
0.6%  cfree
0.6%  llvm::ValueHandleBase::RemoveFromUseList()
0.5%  llvm::SmallPtrSetImplBase::doFind(void const*) const
0.5%  llvm::DominatorTreeBase<llvm::BasicBlock, false>::dominates(llvm::BasicBlock const*, llvm:
0.5%  llvm::LazyValueInfoImpl::getBlockValue(llvm::Value*, llvm::BasicBlock*, llvm::Instruction*
0.5%  llvm::FindFunctionBackedges(llvm::Function const&, llvm::SmallVectorImpl<std::pair<llvm::B
0.5%  llvm::ValueHandleBase::AddToUseList()
0.5%  llvm::Instruction::isDebugOrPseudoInst() const
0.4%  llvm::Value::getName() const
0.4%  llvm::SmallPtrSetImplBase::Grow(unsigned int)
0.4%  llvm::DataLayout::getAlignment(llvm::Type*, bool) const
```

**llvm.codegen: 95.5-159.3s, 12470 samples (~62.7 s CPU)**; libLLVM.so.21.1 81%, libc.so.6 17%, [kernel] 1%, libstdc++.so.6.0.35 1%, ld-linux-x86-64.so.2 0%

```text
51.4%  [unknown]
1.3%  llvm::SelectionDAG::Combine(llvm::CombineLevel, llvm::BatchAAResults*, llvm::CodeGenOptLev
1.3%  llvm::SelectionDAGISel::SelectCodeCommon(llvm::SDNode*, unsigned char const*, unsigned int
1.2%  [kernel]
1.0%  malloc
0.9%  llvm::LiveRegUnits::stepBackward(llvm::MachineInstr const&)
0.8%  llvm::SmallPtrSetImplBase::insert_imp_big(void const*)
0.8%  llvm::TargetLowering::SimplifyDemandedBits(llvm::SDValue, llvm::APInt const&, llvm::APInt
0.8%  cfree
0.7%  llvm::MachineInstr::addOperand(llvm::MachineFunction&, llvm::MachineOperand const&)
0.6%  llvm::PMDataManager::initializeAnalysisImpl(llvm::Pass*)
0.6%  llvm::TargetRegisterInfo::getMinimalPhysRegClass(llvm::MCRegister, llvm::MVT) const
0.5%  llvm::FoldingSetBase::FindNodeOrInsertPos(llvm::FoldingSetNodeID const&, void*&, llvm::Fol
0.5%  llvm::MachineInstr::addRegisterKilled(llvm::Register, llvm::TargetRegisterInfo const*, boo
0.4%  llvm::SelectionDAG::AssignTopologicalOrder()
0.4%  llvm::ScheduleDAGSDNodes::BuildSchedUnits()
0.4%  llvm::SelectionDAG::computeKnownBits(llvm::SDValue, llvm::APInt const&, unsigned int) cons
0.4%  llvm::SelectionDAG::createOperands(llvm::SDNode*, llvm::ArrayRef<llvm::SDValue>)
0.4%  llvm::TargetLowering::SimplifySetCC(llvm::EVT, llvm::SDValue, llvm::SDValue, llvm::ISD::Co
0.3%  llvm::SelectionDAG::Legalize()
```

**link+tail: 159.3-161.2s, 341 samples (~1.7 s CPU)**; [kernel] 26%, libbfd-2.46-system.so 25%, libc.so.6 18%, libLLVM.so.21.1 15%, F-whole 14%

```text
26.4%  [kernel]
24.9%  [unknown]
7.3%  bfd_link_hash_traverse
4.7%  bfd_hash_lookup
4.7%  bfd_elf_link_add_symbols
4.1%  llvm::Value::getValueName() const
3.5%  llvm::BasicBlock::dropAllReferences()
3.5%  fib.drop-one
3.5%  drop.5
3.2%  llvm::StringMapImpl::RemoveKey(llvm::StringMapEntryBase*)
2.1%  fib.release-slow
1.2%  drop.8
1.2%  drop.14
1.2%  drop.965
0.9%  cfree
0.9%  void llvm::SymbolTableListTraits<llvm::Instruction, llvm::ilist_iterator_bits<true>, llvm:
0.9%  llvm::BasicBlock::~BasicBlock()
0.9%  bfd_hash_insert
0.6%  llvm::Value::~Value()
0.6%  drop.684
```


Notes from the call chains:

- **front** is memory management: `fib.drop-one`, `fib.release-slow` and `drop.N` walkers are about 25% of the samples, then map/vec primitives from the prelude. No LLVM.
- **lir.emit** is 31% inside the ORC JIT (`LLVMOrcLLJITLookup` -> `IRCompileLayer` -> `SelectionDAGISel`): the macro runner compiles its code with LLVM on every run, about 4 s of the 14 s of CPU. A cache of the macro runner's objects is the cheapest remaining lever on the serial part.
- **llvm.optimize**: 79% in the inliner and the CGSCC pass manager (InstCombine 24%, GVN 10%, `InlineFunction` 7.5%, `CloneAndPruneFunctionInto` 5.6%).
- **llvm.codegen**: 99.9% in `LLVMTargetMachineEmitToMemoryBuffer`, SelectionDAG 39%, register allocation 6%.
- **link and tail**: 29% of the last 1.7 s is `LLVMDisposeModule` (module teardown) and the rest GNU ld.

## Units

The partitioner makes 19 units: 16 hash groups of code, `glue` (the globals), `runtime` (`fib.*`) and `walkers` (`drop.*`, `trace.*`, `share.*`). Items and definitions per unit, and compile time per unit (8 workers, so these include contention):

| unit | items | definitions | compile ms (min .. max over 3 builds) |
|---|---:|---:|---:|
| 16 code groups | 3300 to 3950 | 695 to 815 | 1100 to 1900 |
| glue | 19,181 | 0 (globals only) | 340 |
| runtime | 410 | 244 | 500 |
| walkers | 17,565 | 12,813 | 4400 to 4570 |

The walkers unit is the critical path of the compile stage: 4.5 s against 1.1 to 1.9 s for the rest. Serial compile of all 19 units takes about 26 s (`h16j1`: 50.4 s wall total), against 76 s for the same code as one module, so partitioning is a 3x saving in CPU before any parallelism.

## Experiment

`matrix.py`: 3 builds then 3 `emit compiler/fibc.fib` timings per config, builder = whole-program F, everything built from one frozen copy of the tree; wall min / median. Builds ran at load 3 to 10 (some at 20 to 33, for `i160` and `i80ns3x32`: their build times are inflated and should be read as upper bounds). `emit` is the speed of the resulting compiler, timed again at the end at load 3 to 7 for the rows marked `*`. The h4..ns3x64 builds are from the tree just before the copy knob existed (identical partition code), their emit was re-timed on the frozen input at load 3 to 4.

| config | description | units | jobs | build wall min / med s | build CPU s | peak RSS MiB | exe MiB | 3 builds identical | emit min / med s | emit vs whole |
|---|---|---:|---:|---:|---:|---:|---:|:-:|---:|---:|
| `whole` | whole program | 1 | - | 101.0 / 101.2 | 100 | 2075 | 11.1 | yes | 23.0 / 23.5 *| +0% |
| `h16j1` | 16 hash groups, serial | 19 | 1 | 50.4 / 50.5 | 46 | 1270 | 7.0 | yes | 30.7 / 30.8 | +31% |
| `h16` | 16 hash groups | 19 | 8 | 34.7 / 35.3 | 54 | 1359 | 7.0 | yes | 31.5 / 33.4 *| +42% |
| `h4` | 4 hash groups | 7 | 8 | 30.7 / 30.7 | 50 | 1515 | 7.2 | yes | 30.4 / 30.6 | +30% |
| `h8` | 8 hash groups | 11 | 8 | 33.7 / 34.5 | 53 | 1432 | 7.1 | yes | 30.3 / 30.6 | +30% |
| `h32` | 32 hash groups | 35 | 16 | 30.0 / 30.7 | 52 | 1389 | 7.0 | yes | 29.8 / 29.8 | +27% |
| `h64` | 64 hash groups | 67 | 24 | 30.2 / 30.2 | 54 | 1389 | 7.0 | yes | 30.4 / 30.8 | +31% |
| `ns2x16` | namespace(2), 16 groups | 19 | 8 | 30.5 / 31.2 | 50 | 1442 | 7.3 | yes | 29.2 / 29.5 | +25% |
| `ns2x32` | namespace(2), 32 groups | 35 | 16 | 30.1 / 30.4 | 52 | 1456 | 7.3 | yes | 29.2 / 29.3 | +24% |
| `ns3x32` | namespace(3), 32 groups | 35 | 16 | 30.4 / 30.5 | 51 | 1402 | 7.1 | yes | 29.7 / 29.8 | +26% |
| `ns3x64` | namespace(3), 64 groups | 67 | 24 | 30.5 / 30.6 | 55 | 1410 | 7.1 | yes | 28.9 / 29.4 | +25% |
| `i20` | 16 hash + copies of callees <= 20 nodes | 19 | 8 | 35.2 / 35.8 | 55 | 1395 | 7.5 | yes | 31.2 / 31.4 *| +34% |
| `i40` | 16 hash + copies <= 40 | 19 | 8 | 38.7 / 39.0 | 81 | 1482 | 9.9 | yes | 27.5 / 29.5 *| +26% |
| `i80` | 16 hash + copies <= 80 | 19 | 8 | 48.8 / 49.8 | 148 | 1731 | 15.5 | yes | 24.9 / 25.1 *| +7% |
| `i160` | 16 hash + copies <= 160 | 19 | 8 | 58.5 / 63.3 | 213 | 1875 | 18.4 | yes | 24.1 / 24.6 *| +5% |
| `i80ns3x32` | namespace(3) x 32 + copies <= 80 | 35 | 16 | 49.6 / 69.3 | 199 | 1792 | 14.3 | yes | 24.0 / 24.2 *| +3% |


Every config emitted identical lIR (md5 aee45d6c...). Every config built three byte-identical executables, with the same output name.

How to read it:

- **Unit count and split do not matter for speed.** 4 to 64 hash groups all land 27 to 42% slower than whole-program (the spread is noise at load 3 to 7) for the compiler; build wall is 30 to 35 s for all of them. By namespace (`ns2`, `ns3`) the loss is 24 to 26%, a small gain from keeping a namespace together. Above 4 units the build is bound by the serial 23 s plus the walkers unit, not by how many workers there are.
- **The loss is in what is separated, not in how finely the code is cut.** The hot callees are the runtime (`fib.*`), the walkers and prelude specialisations; no code split puts them next to every caller. Copying the small ones into each unit does (`i80`).
- **Threshold.** 20 and 40 nodes give little (+34%, +26%); 80 nodes gives +7% on the compiler for about 1.5x the build CPU; 160 reaches whole-program speed on the compiler (+5%) but costs 213 s of CPU and 1.9 GiB, with a 18 MiB executable and 58 to 63 s wall: it is then not much cheaper than whole-program on a machine with fewer cores.
- Parallelism: 1 job 50.5 s, 8 jobs 35.3 s. More workers do not help (h32/16 jobs 30.7, h64/24 jobs 30.2): the unit stage is already about 5 s.

### User programs

Small programs (`scripts/perf/progs.py`, whole-program-built F, build 3x and run 3x, min / median, load 3.4 to 4.3):

| program | mode | build s | run s |
|---|---|---:|---:|
| num-f64 | whole | 0.32 / 0.32 | 1.73 / 1.74 |
| | 19 units, serial, cold cache (today's default) | 0.58 / 0.61 | 6.15 / 6.21 |
| | 19 units, 8 jobs, cache off | 0.45 / 0.46 | 6.17 / 6.40 |
| | + copies <= 80 | 0.49 / 0.49 | 1.69 / 1.70 |
| lazy-fused | whole | 0.40 / 0.42 | 0.83 / 0.83 |
| | 19 units, serial, cold cache | 0.64 / 0.64 | 1.78 / 1.88 |
| | 19 units, 8 jobs, cache off | 0.45 / 0.46 | 1.78 / 1.82 |
| | + copies <= 80 | 0.50 / 0.52 | 0.96 / 0.97 |
| strings | whole | 0.79 / 0.80 | 0.72 / 0.73 |
| | 19 units, serial, cold cache | 0.87 / 0.89 | 0.90 / 0.96 |
| | 19 units, 8 jobs, cache off | 0.56 / 0.57 | 0.91 / 0.91 |
| | + copies <= 80 | 0.67 / 0.68 | 0.77 / 0.78 |

`scripts/bench/quick.sh` with the same F, programs built by the default `fibc build` (cache on, so 19 units) vs `FIB_LIRO_MODE=whole` (medians of 3, seconds; the checked-in baseline.tsv is from 2026-10-04, so compare columns, not the baseline):

| bench | default (units) | `FIB_LIRO_MODE=whole` | `FIB_LIRO_IMPORT=80` | `FIB_LIRO_IMPORT=20` |
|---|---:|---:|---:|---:|
| num-f64 | 6.03 | 1.69 | 1.75 | 1.67 |
| num-nbody | 1.42 | 0.74 | 0.72 | 1.13 |
| vec-conj-pop | 1.77 | 0.92 | 0.99 | 1.45 |
| vec-index | 0.48 | 0.40 | 0.53 | 0.53 |
| vec-sort | 0.68 | 0.46 | 0.51 | 0.67 |
| map-assoc-get | 0.52 | 0.32 | 0.35 | 0.44 |
| set-conj | 0.40 | 0.31 | 0.28 | 0.37 |
| lazy-fused | 1.91 | 0.82 | 0.99 | 1.88 |
| lazy-bound | 0.09 | 0.03 | 0.04 | 0.08 |
| strings | 0.91 | 0.75 | 0.79 | 0.79 |

All checksums matched. The same F built partitioned (`o0/F-h16`) gave the same rows as the whole-built F (num-f64 5.85 vs 6.03): the speed of programs a compiler makes does not depend on how the compiler was built, only on how the program is built; the quick benchmark therefore cannot distinguish a partitioned-built F from a whole-built one, and `F emit` is the test of F's own speed.

Findings for small programs: a cold partitioned build is slower (0.6 s) than whole-program (0.3 s) because the front end dominates and 19 objects cost more than one; units pay off only for large programs or a warm cache. And today's default (cache on, units) runs `num-f64` 3.6x slower than whole-program. That is a question for ADR 0024's default, separate from release builds.

## Fixed point

`FIB_LIRO_CACHE=off FIB_LIRO_PARTITION=1 FIB_LIRO_JOBS=8 F build compiler/fibc.fib ...` with F itself built the same way: F3 36.65 s wall, 53.8 s CPU, 1386 MiB at load 2.6. `sha256sum F F3` equal (5c354c57...), `F emit` and `F3 emit` md5 equal. Whole-program F3: 100.8 s, 2123 MiB, executable equal to F.

## Recommendation

1. **Release builds stay whole-program** (the shipped compiler is 30 to 40% faster than a 16-unit one, and `i80` costs 14 s and 15 MiB for 7% back).
2. **Bootstrap and fixed-point builds can be partitioned and parallel.** The fixed point compares lIR and executables, and both are identical; the stage compilers F and F3 are used only to build and to emit, so a 30% slower F costs less than the 66 s saved per build (101 s to 35 s; the seed build of F, the F3 build and the CI gate each use it). Proposed: `FIB_LIRO_PARTITION=1 FIB_LIRO_JOBS=8` for the F and F3 builds in `mk/stage2.mk` and `scripts/`, with the final released `F` rebuilt whole-program from the same lIR. Not done here: it changes the build recipes, which the lead gates.
3. **Make the opt-in a supported mode:** `--jobs N` for `build` with the cache off, and parallel misses with the cache on (this prototype compiles in parallel only with the cache off, because the cache counters and publication use unsynchronised cells).
4. **Consider `FIB_LIRO_IMPORT=80` for the default unit build** (or whole-program for small programs). It removes the 1.3x to 3.6x slowdown of default builds for about 1.5x build CPU and about 1.4x link size. Risk: a copy has its own address; pointer identity of function values taken in a copy is not preserved (the glue unit is excluded, and every test and benchmark passed, but there is no test that compares function addresses).
5. Next levers, in order of expected gain: split the walkers unit (the critical path of the compile stage, 4.5 s); cache the macro runner's JIT objects (about 4 s of the serial 22 s); cut reference-count traffic in the front end (25% of its samples); the front end and emitter are then the floor (about 23 s).

## Not measured, caveats

- `perf` needed `kernel.perf_event_paranoid` lowered from 4 to 1 (sudo, this box) for the profile run; it was set back to 4.
- libLLVM is stripped of internal symbols, so half of the LLVM leaf samples are unnamed; inclusive views are from partial frame-pointer chains.
- The profile run itself (161 s wall) ran at load 11 to 22; use the matrix for times. `i160` and `i80ns3x32` builds ran at load up to 33.
- A second machine (the Ryzen 5800X) is not in this document; see the commit message.
- Whole-program builds are deterministic only for a fixed output name: the module name (the `FILE` symbol) is the output file name, so `-o F1` and `-o F2` differ in 8 bytes of the symbol table. Partitioned builds name modules `liro-unit-N` and do not have the dependence.
- Not tried: ThinLTO-style summary-based importing instead of source-level copies, splitting by call-graph affinity, and the opt-in with the cache on.
