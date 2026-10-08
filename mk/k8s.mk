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
K8S_TAG := $(shell sha256sum k8s/ci.Dockerfile 2> /dev/null | cut -c1-12)
K8S_IMAGE := fibber-ci:$(K8S_TAG)
K8S_DIR := $(BUILD)/k8s
K8S_CPU ?= 12
K8S_MEM ?= 24Gi
K8S_J ?= 12
K8S_TTL ?= 3600
K8S_DEADLINE ?= 14400
K8S_RUN := $(shell date +%s)
K8S_KUBECTL := kubectl -n $(K8S_NS)
K8S_LOADER := loader-$(K8S_ARCH)
K8S_SEED_HOST ?= $(FIB_SEED_CACHE)/$(SEED_SHA)/seed.tar.gz
K8S_GATE_CMD = make -k -j$(K8S_J) CASE_JOBS=$(CASE_JOBS) gate-stamps || true; make gate-report
K8S_QUICK_CMD = make -k -j$(K8S_J) CASE_JOBS=$(CASE_JOBS) quick-stamps || true; make gate-report GATE_MODE=quick
K8S_NAME ?= x

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
  --arg pvc work-$(K8S_ARCH) -f k8s/job.jq | $(K8S_KUBECTL) apply -f -; \
t0=$$(date +%s); v=; while [ -z "$$v" ]; do sleep 5; \
  st=$$($(K8S_KUBECTL) get job $$name -o jsonpath='{.status.succeeded}/{.status.failed}/{.status.active}'); \
  case $$st in [1-9]*/*/*) v=PASS ;; */[1-9]*/*) v=FAIL ;; esac; \
  if [ -z "$$v" ] && [ $$(( $$(date +%s) - t0 )) -gt 30 ] && [ -z "$${st##//}" ]; then \
    ev=$$($(K8S_KUBECTL) get events --field-selector involvedObject.name=$$name,reason=FailedCreate -o jsonpath='{.items[-1].message}'); \
    if [ -n "$$ev" ]; then echo "$$ev"; v=REJECTED; fi; fi; done; \
$(K8S_KUBECTL) logs job/$$name --all-containers --tail=-1 > $$log 2>&1 || true; tail -n 25 $$log; \
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
	$(call k8s_run,job,make -j$(K8S_J) CASE_JOBS=$(CASE_JOBS) $(TARGET))
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
.PHONY: k8s-apply k8s-image k8s-src k8s-seed k8s-gate k8s-quick k8s-mutants k8s-job k8s-cron-apply k8s-status k8s-clean k8s-clean-all
