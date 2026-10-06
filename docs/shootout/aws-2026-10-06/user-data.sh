#!/bin/bash
# dead-man timer: the instance powers off (and, with shutdown behaviour terminate, is terminated) 150 minutes after boot
shutdown -h +150
export DEBIAN_FRONTEND=noninteractive
{ apt-get update && apt-get install -y --no-install-recommends openjdk-21-jdk-headless gcc libc6-dev libgmp-dev python3-numpy libblas3 libopenblas0-pthread time
  update-alternatives --set libblas.so.3-$(uname -m)-linux-gnu /usr/lib/$(uname -m)-linux-gnu/blas/libblas.so.3; } > /var/log/bench-setup.log 2>&1
echo $? > /var/tmp/setup-done
