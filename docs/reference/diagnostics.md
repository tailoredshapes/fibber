# Diagnostics

Use `fibc explain FILE` for ownership decisions and `fibc check MODULE..` for
all-definition checking. Check exits 0 on acceptance, 3 on rejection, 2 when a
module cannot load. `fibc test` exits 0 when all scenarios hold, 1 for scenario
failure and 2 for usage/discovery/compilation failures. Full command help is in
[the CLI reference](cli.md).

Messages such as “passed to more than one & parameter” describe in-out aliasing;
“cell cannot be shared between threads” describes a non-Send capture. Bounds and
integer-overflow traps are runtime errors. A link failure names the missing
native library; a CPU-floor trap lists missing instructions. Message wording
is diagnostic information, not a promised stable API; prefer typed errors and
exit codes for programmatic handling.
