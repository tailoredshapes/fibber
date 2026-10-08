# The build: GNU Make coordinates, targets are files

Status: adopted (package MAKE-1, 2026-10-07). The owner: "a menagerie of shell scripts; make Make the build coordinator of
choice; classic make files that target file creation".

## 1. What this is

`Makefile` at the root and `mk/*.mk` (one section each, under 300 lines) are the coordinator. Every stage of the gate, the
release and the tool fetches is a **file**: a built program (`build/F`), a generated source (`compiler/emit/runtime.fib`), a
table of results (`build/cases/stdlib.3.txt`), or a stamp (`build/golden/own-amp.ok`) that holds the seconds the stage took
and exists only when the stage passed on the inputs it names. `make -j8 gate` is the full gate; it is incremental by
construction, because Make compares the stamps with their inputs: a change to one mutant script reruns one stamp, a change to
`lib/fib/json/parse.fib` rebuilds `build/F` and reruns everything downstream of F (which is everything: that is correct), a
change to nothing is a no-op.

GNU Make 4.x (4.4.1 on the box). macOS has BSD make: install `gmake` from Homebrew and run `gmake`; the Makefile refuses other
makes (`mk/config.mk` checks `$(MAKE_VERSION)`).

## 2. The graph

```
SEED ─────────────────────► build/seed/<sha256>/bin/fibc        fetch (scripts/fetch-seed.sh), sha256 verified, unpacked
rt/*.lir + gen-runtime ───► compiler/emit/runtime.fib           generated (a real target; build/runtime-drift.ok compares)
compiler/**/*.fib lib/**  ► build/F                             stage 2, built by the seed (BUILDER=… names another fibc)
build/F ──────────────────► build/F.lir, build/F3, build/F3.lir ► build/fixed-point.ok   (the stage check: cmp of the two emits)
build/F ──────────────────► build/golden/tools/<tool> ─────────► build/golden/<suite>.ok  (one per suite of compiler/tests/golden/suites.sh)
build/F + cases/<dir>/** ─► build/cases/<dir>.<k>.txt ─────────► build/cases/full.ok      (the comparison with scripts/ci-stage2.expected and the floor)
                                                          └────► build/cases/quick.ok     (ownership, modules, the stdlib sample)
build/F + specs/** ───────► build/specs.ok
build/F + a tool script ──► build/tools/<name>.ok               (one per entry of the old scripts/tools.sh queue)
build/F + the tree ───────► build/adr.ok                        (the executable ADRs, --strict)
build/F + musl pieces ────► build/static.ok                     (SKIPPED when there are no musl pieces: FIB_MUSL_DIR or build/musl/<arch>/)
build/F + wasi-sdk ───────► build/wasm.ok                       (SKIPPED when there is no node or no wasi-sdk)
VERSION + version.fib ────► build/version.ok
all of the above ─────────► gate                                (prints the summary; GATE PASS (full) / GATE FAIL (full))
build/F + scripts/mutant-X.sh + the files it plants into ► build/mutants/<X>.ok   (`make mutants`; not in the gate)
build/F + scripts/bench/*.fib ► build/bench/quick.tsv            (`make bench`: scripts/bench/quick.sh against baseline.tsv)
seed + llvm static archives ► build/release/F, F3, fibc (shipped) ► dist/fibc-VERSION-PLATFORM.tar.gz, dist/SHA256SUMS   (`make release`)
scripts/fetch-*.sh ───────► ~/.cache/fibber-scratch/tools/<tool>/… (`make fetch-tools`; each fetch checks the sha256 written in its script)
```

`make help` lists the public targets (the `## ` comments of the rules). `make -n gate` shows what would run.

## 3. Decisions

- **Stamps carry data and are written last.** A stamp's recipe removes the old stamp first, runs the stage with its output in
  `<stamp>.log`, and writes the elapsed seconds into the stamp only when the stage passed (`mk/config.mk`, `stamp`). The shell is
  `bash -eu -o pipefail`, so a failing command fails the recipe; `.DELETE_ON_ERROR` removes a target a failed recipe changed.
- **No `.ONESHELL`.** Each recipe is one shell line (continued with `\`), or calls a kept script. `.ONESHELL` would make every
  recipe in the tree one script, where a failed line in the middle is only caught with `-e` on every recipe; the per-line
  model is the classic one and the kept scripts hold the long logic.
- **`.SECONDARY` with no prerequisites:** no intermediate is deleted (the shard tables, the emits, the golden tools are what you
  read when a stage fails).
- **Parallelism is Make's jobserver.** `make -jN` replaces `scripts/lib/slots.sh` (the slot files) and `GATE_BUDGET`: `scripts/gate.sh`
  passes `-j$GATE_BUDGET`. A recipe holds one job slot. Inside a recipe, `fibc cases -j $(CASE_JOBS)` (default 4) overlaps the
  runs of a shard as before, and a tool script's own parallelism is its own: the kept scripts are not jobserver-aware, and the
  policy is that a recipe is one heavy process plus whatever that process always ran. The stdlib cases are `SHARDS` (default 16)
  targets, ownership 4, so Make schedules them.
- **Recursion:** `gate` and `quick` call `$(MAKE) gate-report` after their stamps, and `gate-report` asks `$(MAKE) -q` whether every
  stamp of the mode is current (so the report prints PASS or FAIL even after a `-k` run that left stamps missing). `scripts/gate.sh`
  and `scripts/batch.sh` call make. Nothing else recurses.
- **Scratch outside the tree.** `TMPDIR` and the tool scripts' scratch are under `SCRATCH` (default `~/.cache/fibber-scratch/mk-<hash of the
  tree path>`), not under `build/`: with `TMPDIR` inside the worktree `cases/stdlib/8283-str-replace-...` fails in a shard (`expected 0, got
  4294967296`) and passes with `TMPDIR` anywhere else (found while writing this; reported, not explained). A tool that opens a Unix socket
  also needs a short path (`fibc serve`: 100 bytes), which is why the hash is short.
- **Configuration is a file too.** `build/static.cfg` and `build/wasm.cfg` hold the musl library and the wasi-sdk the stamp was made for
  (written when `make` reads the Makefile, only if the content changes), so a stamp that said SKIPPED is redone once the pieces exist.
  Consequence: running make with and without `FIB_MUSL_DIR` alternately reruns the static stage each time.
- **The seed is a path.** `build/seed/<sha256>/bin/fibc`: SEED is read for the sha256 and url, and is no prerequisite, so touching SEED
  without changing it rebuilds nothing; a new sha256 is a new directory and a rebuild of F.
- **Build directory:** `build/` in the tree (`BUILD=…` moves it), `dist/` for the release; both ignored by git and skipped by
  the ADR scan (`lib/fib/test/arch/repo.fib`). The seed is unpacked under `build/seed/<sha256>/`; downloads are cached across
  worktrees in `FIB_SEED_CACHE` (default `~/.cache/fibber-scratch/seeds`).
- **`GATE_FRESH=1`** (or `make clean-build`): `build/F` gets a `FORCE` prerequisite, so it is rebuilt from the seed and everything
  downstream reruns. The default builder is always the seed, as CI's is; `BUILDER=path/to/fibc` names another.
- **Downloads** stay in the fetch scripts (the recipe body), each with its checksum, so ADR 0020 holds; the lint of ADR 0020 and
  `scripts/lint-pipefail.sh` read `Makefile` and `mk/*.mk` too (a recipe line is a live line, whatever its leading tab, `@`, `-`).
- **macOS (MAC-1):** `gmake mac-check` (`mk/mac.mk`) is the gate with the stages, tools and golden suites that cannot run on a Mac left out, each
  with its reason in `compiler/tests/expected-macos.txt`, plus `scripts/mac-check.sh --with-f F` (the machine's own checks). GNU Make 3.81
  (`/usr/bin/make`) stops in `mk/config.mk` with a message. The scripts assume GNU coreutils; on Darwin `mk/config.mk` puts `scripts/portable/bin`
  first on `PATH` (`timeout`, `flock`, `nproc`, `sha256sum`, `sha1sum`, `md5sum`, `sed -i`, `date +%N`, `make` = gmake) and `scripts/portable/bash-env.sh` in
  `BASH_ENV` (a `ulimit -v` that succeeds: macOS has no RLIMIT_AS). `scripts/lint-portable.sh` (a tool of the quick gate) fails on every other
  Linux-only tool or bash 4 feature unless the line says `# linux-only: reason`. The cases that fail only on a Mac are
  `scripts/ci-stage2.expected-darwin`; `LLVM_LIBDIR` defaults to Homebrew's `llvm@21`.
- **The generated runtime:** `compiler/emit/runtime.fib` is a real target of `rt/*.lir` and `build/gen-runtime` (built by the seed:
  the generator uses the library only); the recipe writes it only when the content changed, so a checkout whose `rt/` is newer
  than the committed file does not rebuild F for nothing. `build/runtime-drift.ok` is the comparison (the committed file equals
  what the generator produces); `compiler/tests/emit/runtime.sh` (the tool `sh-emit-runtime`) still runs unit-runtime.

## 4. Scripts: retired, wrapped, kept

| Script | Now |
|---|---|
| `scripts/gate.sh` | thin wrapper, still takes `/tmp/fibsuite.lock`: `--full` = `make -k -j$GATE_BUDGET gate-stamps` then `make gate-report`; `--quick` the same on `quick`; prints the same summary lines (`build:`, `cases:`, `== timing`, `GATE PASS (full)`) |
| `scripts/tools.sh` | wrapper: its queue is `mk/tools.mk` (`make tools`, `make tools-quick`, `make build/tools/NAME.ok`); prints the same `ok NAME N s` / `FAIL NAME` lines |
| `scripts/lib/slots.sh`, `scripts/lib/fibc-slot.sh` | kept, unused by the gate: without `GATE_SLOTS` they run the command at once; `golden.sh` and old tool scripts still source them. `-j` of make is the budget |
| `scripts/lib/stage2.sh` | kept for `scripts/bench/quick.sh` and `scripts/batch.sh` (the previous-F cache for the bench) |
| `scripts/package.sh` | wrapper over `make release`; its checks are `mk/release.mk` targets (`build/release/binary.ok`, `build/release/tree.ok`) |
| `scripts/fetch-seed.sh`, `scripts/build-musl.sh`, `scripts/llvm-static.sh`, `scripts/fetch-*.sh` | kept as recipe bodies (each checks its checksum) |
| `scripts/ci-stage2.sh` | the comparison recipe: `--from DIR TABLE:CASEDIR:K..` collects the shard tables Make produced and compares them with the expected set and the floor; the old `ci-stage2.sh F` still runs the directories itself |
| `scripts/check-version.sh` | the recipe of `build/version.ok` |
| `scripts/batch.sh` | kept (the lead's integrator); it calls `scripts/gate.sh`, which calls make |
| `scripts/mac-check.sh` | kept; `make mac-check` runs it (gmake on the Mac) |
| `scripts/mutant-*.sh`, `scripts/bench/*`, `compiler/tests/**/*.sh` | kept: they test something; `make mutants`, `make bench`, `make tools` run them |

## 5. Sibling repositories

A library repository (fib-hocon, fib-zlib, …) gets the same shape, small:

```make
# fib-hocon: GNU Make 4 coordinates the tests (the template of fibber's docs/design/build.md 5). Targets are files under build/; a stamp
# (build/X.ok) exists only when its check passed on the inputs it names, so a second `make test` runs nothing. macOS: `brew install make`, then gmake.
#   make test            `fibc test` (the specs), then scripts/test.sh (the differential check and the fuzz run)
#   make fibc            the pinned release (scripts/fetch-fibc.sh: sha256 checked) under build/fibc/<sha256>/bin/fibc
#   make mutants         the planted faults (scripts/mutants.sh)
#   make bench           the 10 MB speed run
# FIBC=/path/to/fibc uses a compiler you have instead of the pinned release (an unreleased fibber: its stage 2 works).
ifeq ($(filter 4.% 5.%,$(MAKE_VERSION)),)
  $(error GNU Make 4 or later is needed (this is $(MAKE_VERSION)); on macOS: brew install make, then gmake)
endif
.DEFAULT_GOAL := help
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
.DELETE_ON_ERROR:
.SECONDARY:
BUILD ?= build
FIBC_SHA := $(shell sed -n 's/^SHA256=//p' scripts/fetch-fibc.sh)
FIBC_DIR := $(abspath $(BUILD))/fibc/$(FIBC_SHA)
FIBC_PINNED := $(FIBC_DIR)/bin/fibc
FIBC ?= $(FIBC_PINNED)
export FIBC
SRC := $(shell find src specs specs-net tools -name '*.fib') deps.fib
CORPUS := $(shell find corpus -type f)

# $(call stamp,CMD): CMD's output in the stamp's .log; the stamp holds the seconds it took and exists only when CMD passed.
define stamp
rm -f $@; t0=$$(date +%s); if { $(1); } > $(@:%.ok=%.log) 2>&1; then echo "$$(( $$(date +%s) - t0 ))" > $@; else echo "FAIL $@ (log: $(@:%.ok=%.log))"; tail -n 20 $(@:%.ok=%.log); exit 1; fi
endef
%/:
	mkdir -p $@

$(FIBC_PINNED): scripts/fetch-fibc.sh
	rm -rf $(FIBC_DIR) && mkdir -p $(FIBC_DIR)/unpack && scripts/fetch-fibc.sh $(FIBC_DIR)/unpack > /dev/null
	mv $(FIBC_DIR)/unpack/fibc-*/* $(FIBC_DIR)/ && rm -rf $(FIBC_DIR)/unpack && touch $@ && $@ --version   # touched: the tarball keeps the release-time mtime
fibc: $(FIBC)   ## fetch the pinned fibc release (sha256 checked), or check the one FIBC names
$(BUILD)/specs.ok: $(FIBC) $(SRC) | $(BUILD)/
	$(call stamp,$(FIBC) test)
$(BUILD)/tests.ok: $(BUILD)/specs.ok scripts/test.sh scripts/check-manifest.sh scripts/compare.py $(CORPUS) | $(BUILD)/
	$(call stamp,scripts/test.sh)
$(BUILD)/mutants.ok: $(FIBC) scripts/mutants.sh $(SRC) | $(BUILD)/
	$(call stamp,scripts/mutants.sh)
test: $(BUILD)/specs.ok $(BUILD)/tests.ok   ## the specs (`fibc test`), then scripts/test.sh: the differential check and 100000 mutated inputs
	@echo "PASS: $$(cat $(BUILD)/specs.ok) s specs, $$(cat $(BUILD)/tests.ok) s tests"
mutants: $(BUILD)/mutants.ok   ## the planted faults: each must be caught by the spec named for it
bench: $(FIBC)   ## the 10 MB speed measurement (HOCON_BENCH=1 scripts/test.sh)
	HOCON_BENCH=1 scripts/test.sh
help:   ## this list
	@grep -h -E '^[a-zA-Z0-9_./-]+:.*## ' $(MAKEFILE_LIST) | sed -E 's/:[^#]*## /\t/' | sort | awk -F'\t' '{ printf "  %-10s %s\n", $$1, $$2 }'
clean:   ## remove build/
	rm -rf $(BUILD)
.PHONY: fibc test mutants bench help clean
```

Applied to fib-hocon as the example (commit f39a538 on its main: `make test` ran the specs in 60 s and scripts/test.sh in 24 s; a second `make test` ran nothing, 0.6 s); the others follow when they are next touched.

## 6. Tests of the build itself

`compiler/tests/make/graph.sh` (the tool `sh-make-graph`, in the quick gate too) runs on a copy of the tree with a fake `F`: it checks that `make
help` lists every public target, that a touched tool script reruns only its stamp, that a touched compiler source reruns the
cases stamp, that a stamp is not written when its command fails, and that a planted missing prerequisite (a stamp without `F`)
would be caught: the test edits the copy's Makefile and asserts the difference. The no-op gate on the real tree (`make gate` a second time: about 2 s) was timed by hand; see the MAKE-1 report.

## 7. Kubernetes: jobs and tests on lilnas's k3s (package K8S-1, 2026-10-08)

The owner: "If you want to push pods for jobs and testing. If you do setup k8s lilnas should be the orchestrator". lilnas already ran a k3s
(v1.34.4+k3s1, one node, control plane, containerd, local-path storage, kube-router network policies); K8S-1 added a namespace to it and
made jobs of the Make gate. Nothing in `gear` or `kube-system` was touched; k3s was not restarted; the only cluster-scoped objects are
the namespace and the PriorityClass `fibber-ci-low`.

### Topology

```
lilnas (192.168.7.42, amd64, 28 threads)   k3s server + agent                      pool amd64   PVC work-amd64   quota-capped
fibber-a64 (Lima VM on the Mac Studio)     k3s agent, aarch64, 8 vCPU / 16 GiB     pool arm64   PVC work-arm64   taint fibber=ci:NoSchedule    label fibber/role=arm64-ci
fibber-ryzen (WSL2 Ubuntu on the 5800X)    k3s agent, amd64, 16 threads / 15 GB    pool bench   PVC work-bench   taint fibber=bench:NoSchedule label fibber/role=bench
```

Everything lives in the namespace `fibber-ci`. A job is a Kubernetes `Job` rendered by `mk/k8s.mk` from `k8s/job.jq` (JSON, so a command needs no
quoting): one container of the CI image (`k8s/ci.Dockerfile`, tag = the first 12 hex digits of the Dockerfile's sha256), the work volume
mounted at `/work`, `cd /work/tree`, the command. `ttlSecondsAfterFinished` is 3600 (`K8S_TTL`), `backoffLimit` 0, `activeDeadlineSeconds`
14400, `priorityClassName: fibber-ci-low`.

**The image** is built on the box that will run it (`make k8s-image`: docker build, then `docker save | k3s ctr -n k8s.io images import -`), so a
single node needs no registry and the pods use `imagePullPolicy: Never`. The Mac's node builds its own (`make k8s-image-arm64`: docker inside the
VM); the Ryzen's came by `docker save | gzip | ssh ... k3s ctr images import` (Docker Desktop's integration is off in that distro). A registry
(`registry@sha256:a3d8aaa6...5373`, a Deployment with a local-path PVC, reached as `localhost:5000` by a hostPort) is the later step when
more than a few nodes need the image; it needs `/etc/rancher/k3s/registries.yaml`, which means restarting k3s, hence not done.
**The tree** is the working tree (tracked and unignored files, their own mtimes so Make sees the stamps as current), streamed by
`make k8s-src` through the loader pod (`tar | kubectl exec -i ... tar -x`; plain `cat` through `exec -i` truncates a large stdin at some 100 KB,
tar does not). **The seed** is copied from the host's seed cache by `make k8s-seed`; the pod's `scripts/fetch-seed.sh` checks its sha256 against
`SEED` (a wrong file fails there). No GitLab credentials or deploy keys exist anywhere in this.

### Running

| Command | What |
|---|---|
| `make k8s-apply` | namespace, quota, LimitRange, RBAC, network policies, the amd64 work volume and loader (`kubectl apply -k k8s/`) |
| `make k8s-gate` | image, tree, seed, then `make fetch-wasm musl` (cached on the volume, so the wasm and static stages run), `make -k -j$K8S_J gate-stamps`, `make gate-report` in one pod |
| `make k8s-quick` | the quick gate; `make k8s-mutants NAME=json` (or `all`); `make k8s-job TARGET=build/tools/lint-pipefail.ok`; `make k8s-run CMD="..."` |
| `make k8s-shards` | a pod builds F, then the stdlib shards as an Indexed Job (`SHARDS` completions, `K8S_PAR` at once) |
| `make k8s-bench` | the quick benchmarks on the bench node (`K8S_POOL=bench`) |
| `K8S_ARCH=arm64 make k8s-gate K8S_EXTRA=BUILDER=/work/cross/fibc` | the same on the aarch64 node (no linux-aarch64 seed exists yet: the cross-built `fibc`, below) |
| `make k8s-status`, `make k8s-clean`, `make k8s-clean-all` | quota use, jobs and pods; delete finished jobs and `build/k8s/*.log`; also the loader pods and the work volumes |
| `make k8s-netpol-test` | the network policy proof (below) |
| `make k8s-cron-apply` | the weekly mutant sweep CronJob, **suspended**: `kubectl -n fibber-ci patch cronjob mutants-weekly -p '{"spec":{"suspend":false}}'` |

Each prints the tail of the job's log, writes the whole log to `build/k8s/<job>.log`, prints `k8s: <job> PASS|FAIL|REJECTED in N s` and exits with
that status. REJECTED is a Job whose pod the quota or the LimitRange refused (the Job controller reports `FailedCreate`; the wait loop deletes the Job).
A pod that fits no node (`FailedScheduling`, e.g. 8 CPUs on the 8-vCPU VM) is not detected: it waits until the 4-hour deadline; give the arm64 node 7.

### Limits and security notes

- **Quota** (`k8s/quota.yaml`): requests and limits 16 CPU / 32 GiB, 100 pods, 10 volumes, 200 GiB; LimitRange default 2 CPU / 4 GiB, max 16 CPU / 32 GiB
  per container. lilnas has 28 threads and 61 GB; the owner's services keep what is left. The three loader pods count 0.3 CPU of the limit.
- **Network** (`k8s/netpol.yaml`): default deny both ways; DNS to kube-dns; TCP 443 to addresses outside 10/8, 172.16/12, 192.168/16, 169.254/16. `make k8s-netpol-test`
  runs a probe pod in `fibber-ci` and the same pod in a scratch namespace without policies. Pods on the other nodes have **no cluster DNS** (pod traffic does not cross
  the NAT, below), which only matters for jobs that download: put what they need on the work volume first.
- **RBAC** (`k8s/rbac.yaml`): the ServiceAccount `fibber-ci` (no token mounted) has a Role in `fibber-ci` only: read pods, logs, configmaps, secrets, claims,
  events; create and delete jobs and pods; exec. No ClusterRole.
- The CI image runs as uid 1000; jobs mount no host path and no cluster token. A job can read nothing outside its volume and the image.
- **Cleanup**: `make k8s-clean-all` removes the namespace's workloads and volumes; `kubectl delete ns fibber-ci` and `kubectl delete priorityclass fibber-ci-low` remove
  everything K8S-1 made on the cluster; `kubectl delete node fibber-a64 fibber-ryzen` and stopping their `k3s-agent` services (below) remove the nodes.

### Adding a node

1. On the node (systemd needed): put the server's node token in `/etc/rancher/k3s/agent-token` (mode 0600) **without printing it or putting it on a command line**:
   `sudo cat /var/lib/rancher/k3s/server/node-token | ssh NODE 'sudo bash -c "umask 077; mkdir -p /etc/rancher/k3s; cat > /etc/rancher/k3s/agent-token"'`.
2. `ssh NODE 'sudo bash -s ARCH NAME ROLE TAINT' < k8s/node-agent.sh` (arguments: `arm64 fibber-a64 arm64-ci fibber=ci:NoSchedule`). It installs the k3s binary of the
   server's version (sha256 in the script), writes `/etc/rancher/k3s/config.yaml` and the `k3s-agent` systemd unit, and starts it. The agent dials `192.168.7.42:6443`
   outward; nothing needs to reach the node.
3. Import the CI image there, `kubectl apply` a volume and loader for the pool (`k8s/arm64.yaml`, `k8s/bench.yaml` are the examples), add a branch to `k8s/job.jq` if the taint is new.

Cross-node pod networking does **not** work and is not needed: the VM sits behind Lima's NAT (192.168.64.2) and WSL2 behind Hyper-V's (172.18.x), lilnas cannot
route to them, and flannel's VXLAN endpoint would have to be the node's LAN address with UDP 8472 forwarded. The API server reaches the kubelets (logs, exec, cp)
through the agents' own outbound tunnel, which is all `make k8s-*` uses. Not tried: `--flannel-backend=wireguard-native` (it changes the server's flannel settings
and restarts k3s) and `--node-external-ip` with a UDP forward.

### Nodes installed (K8S-1)

| Box | What | Version | Why |
|---|---|---|---|
| Mac Studio | `brew install lima` | 2.2.1 | the aarch64 Linux VM |
| Mac Studio (Lima VM `fibber-a64`, Ubuntu 24.04.5 aarch64, vz, 8 CPU / 16 GiB / 80 GiB disk, config `k8s/lima-arm64.yaml`) | docker.io (apt) 29.1.3; k3s binary | 29.1.3; v1.34.4+k3s1 | builds the arm64 image; the agent |
| WSL2 Ubuntu 26.04 on the Ryzen | k3s binary, `k3s-agent.service` | v1.34.4+k3s1 | the bench node |
| lilnas | docker images `ubuntu:26.04`, `busybox:1.37`, `registry:2` pulled; `fibber-ci:*` built; nothing from apt | | the image |

### Measured (K8S-1, 2026-10-08; every number is from a run quoted in the package report)

**amd64 pod (lilnas, 8 CPU / 16 GiB, `make -k -j4 CASE_JOBS=2`)**: cold (`rm -rf build`), `GATE PASS (full)` in 1209 s, including F from the seed (90 s), the fetched wasi-sdk
and the built musl (so `static` 131 s and `wasm` 147 s are real stages, not SKIPPED); `cases: ownership 30s (exit 0), modules 5s (exit 0), stdlib 274s (exit 1)` (the one expected
non-passing case, 1707). The first runs failed for reasons of the image and of oversubscription, not the tree: `lld-21` in the image made `wasm-ld` exist without a
wasi sysroot (the wasm stage ran and failed instead of SKIPPED: removed); Ubuntu's node 22 fails `8251`/`8252` where the host's node 26 passes (the image carries node 26.10.0, sha256-checked);
and `-j12 CASE_JOBS=4` on 12 CPUs timed out `6100` (300 s) and failed `8037`, `8283` (each passes alone in 35 s): 4 to 6 processes per CPU is too many for the timed cases.
**Fan-out** (`make k8s-shards`): the stdlib tables alone take 413 s in one pod (8 CPU); as an Indexed Job of 16 pods at 1 CPU, 6 at a time, 12 shards finished in 270 s and shard 13
was OOMKilled (at 2 GiB and at 3 GiB; the same shard passes in the shared 16 GiB pod), which fails the Job (`backoffLimit: 0`). Six CPUs fanned out are no faster than eight in one pod,
and the quota is the same CPU either way, so the single pod is the default and the fan-out is kept as a target needing `K8S_SHARD_MEM=6Gi`.

**Quota** (a gate pod held 12 of the 16 CPU): `Error creating: pods "probe-quota-6x6s4" is forbidden: exceeded quota: fibber-ci, requested: limits.cpu=8,limits.memory=8Gi,requests.cpu=8,requests.memory=8Gi, used: limits.cpu=13,limits.memory=25088Mi,requests.cpu=12020m,requests.memory=24608Mi, limited: limits.cpu=16,limits.memory=32Gi,requests.cpu=16,requests.memory=32Gi`;
and `maximum cpu usage per Container is 16, but limit is 32` (LimitRange). `make k8s-gate` printed `k8s: fibber-gate-1791459523 REJECTED in 55 s` for the same cause.
**Network policy** (`make k8s-netpol-test`): in `fibber-ci`, `https://github.com` and `https://1.1.1.1` reachable; `http://github.com`, the gear pod (`10.42.0.9:5090`), the gear service, the API service
(`10.43.0.1:443`), metrics-server, `192.168.7.42:6443`, `:22` and the LAN router BLOCKED. The control pod in a scratch namespace without policies reached the gear pod and service (and `http://github.com`):
the policy, not the network, blocked them.
**TTL**: a job with `K8S_TTL=60` finished at 13:28:57 and `kubectl get` found neither the job nor its pod at 13:30:06; the first jobs of the day (TTL 3600) were gone from the list an hour later.
**Other workloads**: `kubectl top node` before: `lilnas 2505m 8% 24279Mi 38%`; with a 12-CPU gate pod running: `16070m 57% 27348Mi 43%`; after: `2226m 7% 23242Mi 36%`. Restart counts before and after:
gear-reviews 8, coredns 4, local-path-provisioner 1, metrics-server 1 (unchanged; sampled every 90 s through the cold gate: gear stayed `2/2 Running`, 8 restarts). The pods are at priority -1000 and never preempt.

**arm64 node (native aarch64 Linux, Ubuntu 24.04 in a vz VM on the M1 Ultra, 7 CPU to jobs)**: there is no linux-aarch64 seed, so the first `fibc` was cross-compiled on lilnas
(`F build compiler/fibc.fib --target aarch64-unknown-linux-gnu --emit obj`, 120 s) and linked in a pod (`gcc ... -lLLVM-21`): `fibc 0.1.12` / `aarch64`. That `fibc` built F in 109 s, and
`make gate` (the same pod command) gave: ownership 372 pass, modules 32 pass, stdlib 1475 cases: 1456 pass, 1 fail (1707, expected), 18 open; `ci-stage2: ok (1 expected non-passing)`; fixed point,
specs, ADRs, `sh-stack-stack`, the other tools and 25 of 28 golden suites pass; `static` passes after `make musl` for aarch64 (musl 1.2.5 tarball put on the volume: pods on this node have no DNS)
and `wasm` is SKIPPED (the wasi-sdk is x86-64 only). The failures are all tests that assume an x86 host, none a runtime fault: `golden/emit-defs`, `emit-fns`, `lair-llvm` (goldens recorded with the x86 triple and
data layout), `tools/sh-driver-muladd`, `sh-driver-no-fma`, `sh-driver-cpu-check`, `sh-lanes-lanes` (`'x86-64-v4' is not a recognized processor for this target`), `js-backend` (`differ cases/lir/simd/target.lir`).
**Weak memory**: the pool cases 8640-8648 at `FIB_THREADS=28` on 7 CPUs, 40 rounds: 40 of 40 passed (about 39 s a round; `k8s/stress-sched.sh`); `scripts/mutant-sched.sh check` (ring overflow, FIB_THREADS=1,2,3: 25 of 25 cases each) passed;
`mutant-sched.sh all`: steal-no-cas, pop-last-no-race, lost-wakeup, park-not-help, run-unclaimed, complete-first KILLED; **pop-fence-release NOT KILLED** (as on x86: "needs a model checker") and **complete-no-fence
NOT KILLED on aarch64 but KILLED on x86** (two `gap` stress runs hang there; the same runs answered right on aarch64 in the VM). So the Chase-Lev orderings have run, unmutated, 40 rounds on a weak-memory machine without a failure, but the
mutation oracle for the fences is weaker there than on x86, not stronger; a bare VM on a busy Mac is also a timing environment, not a verdict on the orderings.

**Bench node (Ryzen WSL2)**: joined (`fibber-ryzen Ready`, taint `fibber=bench:NoSchedule`), image imported, a pod ran on it, then **stopped**: the Windows `C:` drive has 16 MB free (`Get-PSDrive C`: Free 16658432), the
distro's filesystem returns I/O errors (`getpwuid(0) failed 5`, `fopen(/etc/default/locale) failed 5`) and nothing can run in it, so no benchmark ran. The k3s images (about 3 GB), the containerd state and one pod's `build/` are in the distro's
virtual disk and may be part of why `C:` is full; nothing outside WSL was changed. The node object was deleted from the cluster. WSL2 also stops its VM when no `wsl.exe` session is open, which the Make target works
around with an ssh session holding `sleep` for the run. Before using this node again: free space on `C:`, `wsl --shutdown` (stops Docker Desktop's distro too), then inside the distro `k3s ctr -n k8s.io images rm` / `rm -rf /var/lib/rancher/k3s/agent` and
`sudo systemctl disable --now k3s-agent` if the node is not wanted.

### GitLab runner

`k8s/gitlab-runner/values.yaml` is a Helm values file for the `gitlab/gitlab-runner` chart with the Kubernetes executor in `fibber-ci` (low priority class, the CI image, the work volume). **Not installed; it needs the owner
to supply a runner authentication token** (GitLab UI: the fibber project, Settings > CI/CD > Runners > New project runner, tag `k8s`). Steps: the header of that file (create the Secret, allow the GitLab address and port in `k8s/netpol.yaml`, pin the chart version, `helm upgrade --install`).
No GitLab token or deploy key was created.
