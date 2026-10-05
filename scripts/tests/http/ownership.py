"""Validate the native ownership trace without silently accepting missing data."""


def audit_owned_objects(trace):
    lines = trace.splitlines()
    markers = [int(line.split()[1]) for line in lines if line.startswith("I ")]
    marker = markers[-1] if markers else 0
    live = set()
    allocated = 0
    for line in lines:
        fields = line.split()
        if not fields or fields[0] not in ("A", "S", "F", "D"):
            continue
        if len(fields) < 2 or not fields[1].isdecimal():
            continue
        identity = int(fields[1])
        if identity <= marker:
            continue
        if fields[0] in ("A", "S"):
            if identity in live:
                raise AssertionError(f"duplicate live allocation {identity}")
            live.add(identity)
            allocated += 1
        elif identity not in live:
            raise AssertionError(f"free of unowned object {identity}")
        else:
            live.remove(identity)
    if not allocated:
        raise AssertionError("native ownership trace recorded no allocations")
    if live:
        raise AssertionError(f"{len(live)} owned objects remain after HTTP shutdown")
