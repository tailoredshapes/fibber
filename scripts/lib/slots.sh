# scripts/lib/slots.sh: a job server for the gate (sourced, or run as `slots.sh CMD..`). A budget of N slots is N lock files under
# $GATE_SLOTS; a heavy process (a compile, a case shard, a test program) runs while it holds one, so however many stages run at once
# the machine sees at most N heavy processes. Without GATE_SLOTS (a script run alone) nothing is limited and nothing waits.
#   slots_init DIR N     (re)creates the N slot files and exports GATE_SLOTS=DIR
#   slot_acquire         waits for a free slot and holds it in the current shell (descriptor SLOT_FD)
#   slot_release         lets it go
#   slots.sh CMD..       runs CMD at nice level GATE_NICE (default 10: the heavy work yields to the tests that have a clock, to others on the machine) while holding a slot; exit status is CMD's. A command already running under a slot (GATE_SLOT_HELD, set here)
#                        runs at once: a program that a slot holder starts and that starts a fibc again must not wait for a second slot.
slots_init() {
  GATE_SLOTS=$1; export GATE_SLOTS
  rm -rf "$GATE_SLOTS"; mkdir -p "$GATE_SLOTS"
  local i; for ((i = 0; i < $2; i++)); do : > "$GATE_SLOTS/$i"; done
}
slot_acquire() {
  SLOT_FD=
  [ -n "${GATE_SLOTS:-}" ] && [ -z "${GATE_SLOT_HELD:-}" ] || return 0
  local f
  while :; do
    for f in "$GATE_SLOTS"/*; do
      exec {SLOT_FD}> "$f" || return 0
      if flock -n "$SLOT_FD"; then return 0; fi
      exec {SLOT_FD}>&-
    done
    sleep "0.$((RANDOM % 4 + 1))"
  done
}
slot_release() { [ -n "${SLOT_FD:-}" ] && exec {SLOT_FD}>&-; SLOT_FD=; return 0; }
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  slot_acquire
  if [ -n "${GATE_SLOTS:-}" ]; then GATE_SLOT_HELD=1 nice -n "${GATE_NICE:-10}" "$@"; rc=$?; else "$@"; rc=$?; fi
  slot_release
  exit $rc
fi
