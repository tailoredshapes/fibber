# mk/k8s.mk: jobs and tests as Kubernetes Jobs in the namespace fibber-ci of lilnas's k3s (docs/design/build.md, Kubernetes; manifests in k8s/).
#   make k8s-apply          the namespace, quota, limits, RBAC, network policies, work volume, loader pod (kubectl apply -k k8s/)
#   make k8s-gate           the Make gate in one pod (make -j gate-stamps, then the report); k8s-quick the quick gate
#   make k8s-mutants NAME=json   one planted-fault script (build/mutants/NAME.ok); NAME=all runs `make mutants`
#   make k8s-job TARGET=build/tools/lint-pipefail.ok     any make target(s), in a pod
# Each of them: image (docker build, imported into the node's containerd: no registry), source (the working tree, streamed into the work volume),
# seed (copied from the seed cache, sha256 checked by scripts/fetch-seed.sh in the pod), then the Job. The log is build/k8s/<job>.log, the make
# target prints one verdict line and exits with the job's status. K8S_ARCH=arm64 picks the arm64 node (k8s/arm64.yaml; docs/design/build.md).
K8S_NS := fibber-ci
K8S_ARCH ?= amd64
K8S_POOL ?= $(K8S_ARCH)   # amd64 (lilnas), arm64 (the Lima node) or bench (the Ryzen: K8S_ARCH=amd64 K8S_POOL=bench)
K8S_TAG := $(shell sha256sum k8s/ci.Dockerfile 2> /dev/null | cut -c1-12)
K8S_IMAGE := fibber-ci:$(K8S_TAG)
K8S_DIR := $(BUILD)/k8s
K8S_CPU ?= 12
K8S_MEM ?= 24Gi
K8S_J ?= 6
K8S_TTL ?= 3600
K8S_DEADLINE ?= 14400
K8S_RUN := $(shell date +%s)
K8S_KUBECTL := kubectl -n $(K8S_NS)
K8S_LOADER = loader-$(K8S_POOL)
K8S_SEED_HOST ?= $(FIB_SEED_CACHE)/$(SEED_SHA)/seed.tar.gz
K8S_GATE_CMD = $(K8S_PREP) || true; make -k -j$(K8S_J) CASE_JOBS=$(CASE_JOBS) $(K8S_EXTRA) gate-stamps || true; make gate-report $(K8S_EXTRA)
K8S_QUICK_CMD = make -k -j$(K8S_J) CASE_JOBS=$(CASE_JOBS) $(K8S_EXTRA) quick-stamps || true; make gate-report GATE_MODE=quick $(K8S_EXTRA)
K8S_EXTRA ?=   # more make arguments for the job (arm64: BUILDER=/work/cross/fibc, the cross-built seed)
K8S_PREP = $(if $(filter amd64,$(K8S_ARCH)),make fetch-wasm musl,make musl)   # the wasi-sdk and musl pieces (cached on the work volume), so those gate stages run

k8s-apply:   ## fibber-ci on the k3s: namespace, quota, limits, RBAC, network policies, work volume and loader pod (kubectl apply -k k8s/)
	kubectl apply -k k8s

# The image is built here and imported into the node's containerd (k3s ctr): a single node needs no registry. The tag is the hash of the Dockerfile.
$(K8S_DIR)/image-$(K8S_TAG).ok: k8s/ci.Dockerfile | $(K8S_DIR)/
	docker build -f k8s/ci.Dockerfile -t $(K8S_IMAGE) k8s/
	docker save $(K8S_IMAGE) | sudo -n k3s ctr -n k8s.io images import -
	touch $@
k8s-image: $(K8S_DIR)/image-$(K8S_TAG).ok   ## build k8s/ci.Dockerfile with docker and import it into the node's containerd (fibber-ci:<hash of the Dockerfile>)

# The working tree (tracked and unignored files, their own mtimes so that make sees the stamps as current) into the work volume.
k8s-src:   ## stream the working tree into the work volume (/work/tree) through the loader pod
	$(K8S_KUBECTL) wait --for=condition=Ready pod/$(K8S_LOADER) --timeout=300s
	$(K8S_KUBECTL) exec $(K8S_LOADER) -- mkdir -p /work/tree /work/seeds /work/home
	git ls-files -z --cached --others --exclude-standard | tar --null -T - --ignore-failed-read -c 2> /dev/null | $(K8S_KUBECTL) exec -i $(K8S_LOADER) -- tar -x -C /work/tree

k8s-seed:   ## copy the seed tarball from the seed cache into the work volume (the pod's fetch-seed.sh checks its sha256)
	test -f $(K8S_SEED_HOST) || { echo "k8s-seed: no $(K8S_SEED_HOST) (FIB_SEED_CACHE)" >&2; exit 1; }
	# (tar, not cat: kubectl exec -i truncates a large plain stdin at some 100 KB; a tar stream arrives whole)
	tar -C $(FIB_SEED_CACHE) -c $(SEED_SHA) | $(K8S_KUBECTL) exec -i $(K8S_LOADER) -- tar -x -C /work/seeds

# $(call k8s_run,KIND,COMMAND[,CPU,MEM,SHARDS,PARALLELISM]): renders k8s/job.jq, applies it, waits, collects the log, prints the verdict.
# A pod the quota (or the LimitRange) refuses never exists: the Job controller reports FailedCreate, which the wait loop turns into REJECTED.
define k8s_run
name=fibber-$(1)-$(K8S_RUN); log=$(K8S_DIR)/$$name.log; mkdir -p $(K8S_DIR); \
jq -n --arg name $$name --arg kind $(1) --arg arch $(K8S_ARCH) --arg cmd '$(2)' --arg image $(K8S_IMAGE) --arg cpu '$(or $(3),$(K8S_CPU))' \
  --arg mem '$(or $(4),$(K8S_MEM))' --arg ttl $(K8S_TTL) --arg deadline $(K8S_DEADLINE) --arg shards '$(or $(5),0)' --arg par '$(or $(6),0)' \
  --arg pvc work-$(K8S_POOL) --arg pool $(K8S_POOL) -f k8s/job.jq | $(K8S_KUBECTL) apply -f -; \
t0=$$(date +%s); v=; while [ -z "$$v" ]; do sleep 5; \
  st=$$($(K8S_KUBECTL) get job $$name -o jsonpath='{.status.conditions[*].type}/{.status.active}'); \
  case $$st in *Complete*/*) v=PASS ;; *Failed*/*) v=FAIL ;; esac; \
  if [ -z "$$v" ] && [ $$(( $$(date +%s) - t0 )) -gt 30 ] && [ "$$st" = "/" ]; then \
    ev=$$($(K8S_KUBECTL) get events --field-selector involvedObject.name=$$name,reason=FailedCreate -o jsonpath='{.items[0].message}'); \
    if [ -n "$$ev" ]; then echo "$$ev"; v=REJECTED; fi; fi; done; \
$(K8S_KUBECTL) logs -l job-name=$$name --all-containers --tail=-1 --max-log-requests=32 > $$log 2>&1 || true; tail -n 25 $$log; \
[ "$$v" != REJECTED ] || $(K8S_KUBECTL) delete job $$name --wait=false; \
echo "k8s: $$name $$v in $$(( $$(date +%s) - t0 )) s (log $$log)"; [ "$$v" = PASS ]
endef

k8s-gate: k8s-image k8s-src k8s-seed   ## the Make gate in one pod: make -k -j12 gate-stamps, then gate-report (K8S_CPU, K8S_MEM, K8S_J; K8S_ARCH=arm64)
	$(call k8s_run,gate,$(K8S_GATE_CMD))
k8s-quick: k8s-image k8s-src k8s-seed   ## the quick gate in one pod
	$(call k8s_run,quick,$(K8S_QUICK_CMD),8,16Gi)
k8s-mutants: k8s-image k8s-src k8s-seed   ## one planted-fault script in a pod (NAME=json: build/mutants/json.ok; NAME=all: make mutants)
	test -n "$(NAME)" || { echo "k8s-mutants: NAME=json (make mutants-list names them) or NAME=all" >&2; exit 2; }
	$(call k8s_run,mutants,make -k -j$(K8S_J) $(if $(filter all,$(NAME)),mutants,build/mutants/$(NAME).ok))
k8s-job: k8s-image k8s-src k8s-seed   ## any make target in a pod: make k8s-job TARGET=build/tools/lint-pipefail.ok
	test -n "$(TARGET)" || { echo "k8s-job: TARGET=build/F (any make target)" >&2; exit 2; }
	$(call k8s_run,job,make -j$(K8S_J) CASE_JOBS=$(CASE_JOBS) $(K8S_EXTRA) $(TARGET))
# The arm64 node (docs/design/build.md, Kubernetes, arm64): a Lima VM on the Mac. Its image is built there (docker in the VM) and imported into its k3s.
ARM_HOST ?= 192.168.7.254
BENCH_HOST ?= tsmar@192.168.7.83
LIMA_VM ?= fibber-a64
k8s-apply-arm64:   ## the arm64 node's work volume and loader pod (k8s/arm64.yaml); the node itself joins with k8s/node-agent-arm64.sh
	kubectl apply -f k8s/arm64.yaml
k8s-image-arm64:   ## build k8s/ci.Dockerfile inside the Lima VM (ARM_HOST, LIMA_VM) and import it into the VM's k3s containerd
	tar -C k8s -c ci.Dockerfile | ssh $(ARM_HOST) 'export PATH=/opt/homebrew/bin:$$PATH; limactl shell $(LIMA_VM) -- bash -c "mkdir -p ~/ctx && cd ~/ctx && tar -x && sudo docker build -q -f ci.Dockerfile -t $(K8S_IMAGE) . && sudo docker save $(K8S_IMAGE) | sudo k3s ctr -n k8s.io images import -"'
k8s-apply-bench:   ## the benchmark node's work volume and loader pod (k8s/bench.yaml)
	kubectl apply -f k8s/bench.yaml
# On the Ryzen's WSL2 node (quiet x86): builds F, then runs the quick benchmarks (scripts/bench/quick.sh) against the baseline there.
k8s-bench: K8S_POOL := bench
k8s-bench: k8s-image   ## the quick benchmarks on the bench node (BENCH="name.." for some); BENCH_CPU=8 BENCH_MEM=8Gi by default
	@echo "(WSL2 stops its VM when no wsl.exe session is open, which drops the node: an ssh session holding 'sleep' keeps it up for the whole run)"
	ssh -o BatchMode=yes $(BENCH_HOST) 'wsl -u root -e sleep 14400' > /dev/null 2>&1 & kp=$$!; trap "kill $$kp" EXIT; sleep 30; \
	$(MAKE) --no-print-directory K8S_POOL=bench k8s-src k8s-seed && \
	$(call k8s_run,bench,make -j4 CASE_JOBS=4 $(K8S_EXTRA) bench BENCH='$(BENCH)',$(or $(BENCH_CPU),8),$(or $(BENCH_MEM),8Gi))
# The stdlib shards as an Indexed Job (one pod per shard, SHARDS of them, K8S_PAR at a time): F is built first by a plain job on the same volume.
K8S_PAR ?= 8
k8s-shards: k8s-image k8s-src k8s-seed   ## the stdlib case shards as an Indexed Job (SHARDS pods, K8S_PAR at once, K8S_SHARD_CPU/K8S_SHARD_MEM each), after a job that builds F
	$(call k8s_run,prep,make -j$(K8S_J) build/F)
	$(call k8s_run,shards,make CASE_JOBS=1 build/cases/stdlib.$$JOB_COMPLETION_INDEX.txt,$(or $(K8S_SHARD_CPU),1),$(or $(K8S_SHARD_MEM),3Gi),$(SHARDS),$(K8S_PAR))
k8s-run: k8s-image k8s-src k8s-seed   ## any shell command in a pod, in the tree: make k8s-run CMD="build/F cases cases/stdlib --only 8283-str-replace-literal-char-and-regex-on-ascii-and-non-ascii.fib"
	test -n "$(CMD)" || { echo "k8s-run: CMD='shell command' (run in /work/tree)" >&2; exit 2; }
	$(call k8s_run,run,$(CMD))
k8s-cron-apply:   ## the weekly full-mutant-sweep CronJob (suspended: `kubectl -n fibber-ci patch cronjob mutants-weekly -p '{"spec":{"suspend":false}}'`)
	$(K8S_KUBECTL) apply -f k8s/cronjob-mutants.yaml
k8s-status:   ## the namespace's quota use, jobs, pods and volumes
	$(K8S_KUBECTL) describe resourcequota fibber-ci | sed -n '/Resource/,$$p'; $(K8S_KUBECTL) get jobs,pods,pvc -o wide
k8s-clean:   ## delete the finished and failed jobs of fibber-ci (their pods go with them) and the logs under build/k8s/
	$(K8S_KUBECTL) delete jobs --all --ignore-not-found
	rm -rf $(K8S_DIR)/*.log
k8s-clean-all: k8s-clean   ## also the loader pod and the work volumes (the tree, build/, the caches); `make k8s-apply` makes them again
	$(K8S_KUBECTL) delete pod -l fibber/role=loader --ignore-not-found
	$(K8S_KUBECTL) delete pvc --all --ignore-not-found
# The network policy proof: the probe pod in fibber-ci (default deny + DNS + HTTPS) and the control in a scratch namespace without policies.
k8s-netpol-test: k8s-image   ## prove the network policies: a pod in fibber-ci reaches the internet over HTTPS only, not gear, kube-system, the API or the LAN
	gp=$$(kubectl -n gear get pod -o jsonpath='{.items[0].status.podIP}:5090'); gs=$$(kubectl -n gear get svc gear-reviews -o jsonpath='{.spec.clusterIP}:5090'); \
	sed -e "s/IMAGE_TAG/$(K8S_TAG)/" -e "s/GEAR_POD_ADDR/$$gp/" -e "s/GEAR_SVC_ADDR/$$gs/" k8s/netpol-probe.yaml > $(K8S_DIR)/netpol-probe.yaml; \
	kubectl create namespace fibber-ci-control > /dev/null; kubectl -n fibber-ci-control apply -f $(K8S_DIR)/netpol-probe.yaml > /dev/null; \
	$(K8S_KUBECTL) delete pod netpol-probe --ignore-not-found > /dev/null; $(K8S_KUBECTL) apply -f $(K8S_DIR)/netpol-probe.yaml > /dev/null; \
	sleep 45; echo "== fibber-ci (policies)"; $(K8S_KUBECTL) logs netpol-probe; echo "== control (scratch namespace, no policies)"; kubectl -n fibber-ci-control logs netpol-probe; \
	$(K8S_KUBECTL) delete pod netpol-probe > /dev/null; kubectl delete namespace fibber-ci-control > /dev/null

.PHONY: k8s-shards k8s-apply-bench k8s-bench k8s-apply-arm64 k8s-image-arm64 k8s-run k8s-netpol-test k8s-apply k8s-image k8s-src k8s-seed k8s-gate k8s-quick k8s-mutants k8s-job k8s-cron-apply k8s-status k8s-clean k8s-clean-all
