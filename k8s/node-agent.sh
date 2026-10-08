#!/bin/bash
# Joins this machine to lilnas's k3s as an agent (run as root on the node; systemd needed). The server's node token is NOT an argument and is never
# printed: it is read from /etc/rancher/k3s/agent-token, which the owner's session writes with `umask 077` from standard input:
#   sudo cat /var/lib/rancher/k3s/server/node-token | ssh NODE 'sudo bash -c "umask 077; mkdir -p /etc/rancher/k3s; cat > /etc/rancher/k3s/agent-token"'
# usage: node-agent.sh ARCH NAME ROLE TAINT       e.g.  node-agent.sh arm64 fibber-a64 arm64-ci fibber=ci:NoSchedule
#                                                      node-agent.sh amd64 fibber-ryzen bench fibber=bench:NoSchedule
# The k3s binary is the server's release (v1.34.4+k3s1), fetched from the k3s release page and checked against the sha256 written here (docs/adr/0020).
set -eu
arch=${1:?ARCH amd64|arm64}; name=${2:?NAME}; role=${3:?ROLE}; taint=${4:?TAINT}
server=${K3S_SERVER:-https://192.168.7.42:6443}
V='v1.34.4+k3s1'
case $arch in
  arm64) bin=k3s-arm64; sha=2a10a51c9bc04f0f02c2dd52c820399a97656164af7670d530cefd14a27b7b41 ;;
  amd64) bin=k3s; sha=94404b82a83468f5ea058652435f847ea3a7f6fbc3739b9e327e1c6e136db504 ;;
  *) echo "node-agent: ARCH is amd64 or arm64" >&2; exit 2 ;;
esac
test -s /etc/rancher/k3s/agent-token || { echo "node-agent: /etc/rancher/k3s/agent-token is missing (see the header)" >&2; exit 2; }
curl -fsSL -o "/tmp/$bin" "https://github.com/k3s-io/k3s/releases/download/${V/+/%2B}/$bin"
echo "$sha  /tmp/$bin" | sha256sum -c -
install -m 0755 "/tmp/$bin" /usr/local/bin/k3s
rm -f "/tmp/$bin"
cat > /etc/rancher/k3s/config.yaml <<EOF
server: $server
token-file: /etc/rancher/k3s/agent-token
node-name: $name
node-label:
  - fibber/role=$role
node-taint:
  - $taint
EOF
cat > /etc/systemd/system/k3s-agent.service <<'EOF'
[Unit]
Description=k3s agent (fibber CI node)
After=network-online.target
Wants=network-online.target
[Service]
Type=notify
ExecStart=/usr/local/bin/k3s agent
KillMode=process
Delegate=yes
LimitNOFILE=1048576
LimitNPROC=infinity
LimitCORE=infinity
TasksMax=infinity
Restart=always
RestartSec=5s
[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable --now k3s-agent
