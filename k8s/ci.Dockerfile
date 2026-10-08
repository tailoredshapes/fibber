# The CI image: what `make gate` needs on Ubuntu 26.04 (lilnas's own release), for amd64 and arm64 (the base is a multi-arch index).
#   docker build -f k8s/ci.Dockerfile -t fibber-ci:TAG k8s/        (make k8s-image: TAG = the sha256 of this file; the context is k8s/ only)
# The base is pinned by digest (docs/adr/0020). The seed fibc is not in the image: it is checksum-verified by scripts/fetch-seed.sh from SEED
# into the work volume (make k8s-seed), so a new seed does not rebuild the image. The tree is not in the image either (make k8s-src).

# --- base: the toolchain
FROM ubuntu@sha256:f144425ff09be612d6d9ad965196e9cdc23dae1f42110a8a11a3e9a8198759f7 AS base
ENV DEBIAN_FRONTEND=noninteractive
# llvm-21-dev carries the static archives that `llvm-config-21 --link-static` names (scripts/llvm-static.sh, the release); libzstd/zlib/ncurses/xml2
# are what those archives link; gcc for the runtime's C side and the linker; make 4.x (the gate needs it); node for the LSP tests; the rest is what
# the case and tool scripts call (flock, pgrep, perl, xxd ...).
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git openssh-client jq zstd xz-utils bzip2 unzip file patch rsync bc time \
      build-essential make gcc g++ binutils python3 perl \
      llvm-21 llvm-21-dev llvm-21-tools clang-21 libzstd-dev zlib1g-dev libncurses-dev libxml2-dev libedit-dev \
      procps util-linux xxd psmisc iproute2 strace \
 && rm -rf /var/lib/apt/lists/* \
 && ln -s /usr/lib/llvm-21/bin/llvm-config /usr/local/bin/llvm-config-21 \
 && llvm-config-21 --version && make --version | head -1 && gcc --version | head -1

# --- node: the wasm stage's cases need the node the host runs (Ubuntu's own nodejs is 22; 8251/8252 fail there), pinned by sha256 (docs/adr/0020)
FROM base AS node
RUN case "$(dpkg --print-architecture)" in \
      amd64) f=node-v26.10.0-linux-x64.tar.xz; sha=ca70e9e349de048b9522abb3adc05b3bd6f43c5ffd3ec57916c7da292f59f022 ;; \
      arm64) f=node-v26.10.0-linux-arm64.tar.xz; sha=7a6353f63eb3d04765004b4adf172616243e4522434635cb1d26288658b04ab5 ;; \
      *) echo "no node for this architecture"; exit 1 ;; esac \
 && curl -fsSL -o /tmp/node.tar.xz "https://nodejs.org/dist/v26.10.0/$f" \
 && echo "$sha  /tmp/node.tar.xz" | sha256sum -c - \
 && mkdir -p /opt/node && tar -xJf /tmp/node.tar.xz -C /opt/node --strip-components=1

# --- ci: the user and the paths a job uses (the work volume is mounted at /work and holds the tree, build/ and the caches)
FROM base AS ci
COPY --from=node /opt/node /opt/node
RUN id ubuntu >/dev/null 2>&1 || useradd -m -u 1000 ubuntu
ENV HOME=/work/home \
    FIB_SEED_CACHE=/work/seeds \
    PATH=/opt/node/bin:/usr/lib/llvm-21/bin:/usr/local/bin:/usr/bin:/bin
USER 1000
WORKDIR /work/tree
CMD ["bash"]
