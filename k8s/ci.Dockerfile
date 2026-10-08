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
      build-essential make gcc g++ binutils python3 perl nodejs \
      llvm-21 llvm-21-dev llvm-21-tools clang-21 lld-21 libzstd-dev zlib1g-dev libncurses-dev libxml2-dev libedit-dev \
      procps util-linux xxd psmisc iproute2 strace \
 && rm -rf /var/lib/apt/lists/* \
 && ln -s /usr/lib/llvm-21/bin/llvm-config /usr/local/bin/llvm-config-21 \
 && llvm-config-21 --version && make --version | head -1 && gcc --version | head -1

# --- ci: the user and the paths a job uses (the work volume is mounted at /work and holds the tree, build/ and the caches)
FROM base AS ci
RUN id ubuntu >/dev/null 2>&1 || useradd -m -u 1000 ubuntu
ENV HOME=/work/home \
    FIB_SEED_CACHE=/work/seeds \
    PATH=/usr/lib/llvm-21/bin:/usr/local/bin:/usr/bin:/bin
USER 1000
WORKDIR /work/tree
CMD ["bash"]
