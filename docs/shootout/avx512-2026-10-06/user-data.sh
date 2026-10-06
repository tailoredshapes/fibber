#!/bin/bash
# dead-man timer: the instance powers off (and, with shutdown behaviour terminate, is terminated) 120 minutes after boot
shutdown -h +120
export DEBIAN_FRONTEND=noninteractive
{ apt-get update && apt-get install -y --no-install-recommends python3-numpy libblas3 libopenblas0-pthread time util-linux; } > /var/log/bench-setup.log 2>&1
echo $? > /var/tmp/setup-done
