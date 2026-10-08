# The Job template (rendered by mk/k8s.mk: `jq -n --arg name .. -f k8s/job.jq | kubectl apply -f -`; JSON so that a command needs no quoting).
# Arguments: pool (amd64, arm64 or bench: picks the node pool, its taint and its volume) name kind arch cmd image cpu mem ttl deadline parallelism(0 = a plain job) shards(0 = not indexed) pvc
{
  apiVersion: "batch/v1",
  kind: "Job",
  metadata: {name: $name, namespace: "fibber-ci", labels: {"fibber/job": $kind, "fibber/arch": $arch}},
  spec: ({
    ttlSecondsAfterFinished: ($ttl | tonumber),
    backoffLimit: 0,
    activeDeadlineSeconds: ($deadline | tonumber),
    template: {
      metadata: {labels: {"fibber/job": $kind, "fibber/arch": $arch}},
      spec: {
        priorityClassName: "fibber-ci-low",
        restartPolicy: "Never",
        nodeSelector: ({"kubernetes.io/arch": $arch} + (if $pool == "bench" then {"fibber/role": "bench"} else {} end)),
        tolerations: [{key: "fibber", operator: "Equal", value: (if $pool == "bench" then "bench" else "ci" end), effect: "NoSchedule"}],
        securityContext: {runAsUser: 1000, runAsGroup: 1000, fsGroup: 1000},
        containers: [{
          name: "ci",
          image: $image,
          imagePullPolicy: "Never",
          command: ["bash", "-ceu", "mkdir -p \"$HOME\"; cd /work/tree; eval \"$FIB_JOB_CMD\""],
          env: [{name: "FIB_JOB_CMD", value: $cmd}, {name: "FIB_SHARDS", value: $shards}],
          resources: {requests: {cpu: $cpu, memory: $mem}, limits: {cpu: $cpu, memory: $mem}},
          volumeMounts: [{name: "work", mountPath: "/work"}]
        }],
        volumes: [{name: "work", persistentVolumeClaim: {claimName: $pvc}}]
      }
    }
  } + (if ($shards | tonumber) > 0 then {completions: ($shards | tonumber), parallelism: ($par | tonumber), completionMode: "Indexed"} else {} end))
}
