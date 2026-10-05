# OS abstraction

The HTTP implementation exposed libc names, errno numbers, open flags, and
socket layouts that differed across Linux and macOS. `fib.os` owns these
differences and supplies shared services to HTTP, the compiler, and future
libraries. Its [public reference](../../lib/fib/os/README.md) documents the API.

## Layers and backend selection

Applications use named operations, typed options, byte arrays, and
`Result T OSError`. ErrorKind is the portable branch key; numeric errno is
diagnostic data. HTTP maps these errors to its existing error model at one
boundary. File creation flags, nonblocking flags, clocks, socket options,
address layouts, and system structures belong to the OS layer.

Common modules implement validation, allocation bounds, resource cleanup,
retry loops, deadlines, and cancellation. Unsafe POSIX calls remain private
within those modules. `fib.os.native` contains C-string and pointer conversion
helpers for native integrations. `fib.os.abi-data` holds explicit Linux and
Darwin layouts and constants; `fib.os.abi/host` obtains its value from the
selected backend. These internal modules are not re-exported by `fib.os`.

The module loader selects only the internal `fib.os.backend` namespace:

| Compilation host | Relative backend source |
|---|---|
| Linux | `fib/os/backend.fib` |
| Darwin | `platform/darwin/fib/os/backend.fib` |
| Other | `platform/unsupported/fib/os/backend.fib` (currently absent) |

Both backend files declare the same namespace and interface. Selection uses
the compiling host's `uname`; the library does not promise cross-compilation.
Ordinary module search order remains main directory, command-line roots,
FIB_LIB roots, then the built-in library. Unsupported systems fail module
resolution instead of selecting a Linux implementation silently. Loading a
deliberate overlay root is also useful for backend source checks.

The raw backend interface covers ABI data, errno access, clearing errno,
socket creation/acceptance, close-on-exec pipes, directory reads, and executable
discovery. Fallible descriptor operations return negative errno, capturing it
before freeing buffers or closing failed resources. Common wrappers turn this
into OSError. Extending backend operations should preserve this convention.

Linux uses `accept4`, `pipe2`, and socket creation flags to set close-on-exec
atomically. Darwin uses `accept`, `pipe`, and `fcntl`, cleaning up if setup
fails. Those Darwin fallbacks have a descriptor-inheritance race with a
concurrent fork/exec; an application that relies on inheritance isolation must
serialize process creation with descriptor creation until an atomic solution
is available. Configuration errors retain the original errno.

## Services beyond sockets and files

The facade also exports monotonic and wall clocks, interrupted sleep recovery,
environment operations, process and parent IDs, executable and working-directory
discovery, system identity, page size, online CPU count, and secure entropy.
Directory listing and temporary directory creation are shared with the compiler.
These services support schedulers, benchmarks, task pools, caches, and temporary
workspaces without duplicating platform bindings.

Descriptors remain explicit integer resources, with close-on-exec defaults for
new files, pipes, and sockets. The existing language cannot attach automatic
cleanup to an integer. Each successful resource acquisition must therefore
have a matching close on ordinary exits. High-level helpers close on both
success and Result errors. `dup` intentionally retains POSIX inheritance
semantics. Environment and working-directory mutation are process-wide.

TCP helpers handle numeric IPv4 and IPv6 addresses. Shared poll/send loops
bound backpressure with deadlines and cancellation; they require nonblocking
sockets. SIGPIPE is suppressed with the host's MSG_NOSIGNAL. HTTP's transport
adapter now contains protocol error conversion instead of sockaddr layouts or
platform errno numbers.

Bounded native memory streams also belong to this layer. Local closures hide
the FILE pointer, allocation address, and mutable state; their public values
cannot satisfy Send. Closing is idempotent. Exposing a native FILE pointer is
explicit interoperability and requires the caller to obey its lifetime.
This API can support foreign libraries beyond libcurl.

## Compiler integration

The compiler's launcher and linker obtain errno and pipes from the backend.
The case harness uses shared directory listing, temporary directories, and
executable discovery. Installed library discovery no longer contains
`/proc/self/exe`; Linux readlink and Darwin `_NSGetExecutablePath` are backend
operations. Relative executable paths are resolved before use.

The Rust seed and generated runtime template remain frozen. `emit.os/sys-for`
adapts the template when emitting the Darwin runtime: `__errno_location` becomes
`__error`, AT_FDCWD becomes -2, and CLOCK_MONOTONIC becomes 6. The Linux runtime
text remains identical. Cases check all three adaptations and backend paths.
Existing `fib.unix` APIs retain raw errno results; their file flags now use the
shared ABI data.

Two existing compiler limitations influenced the implementation:

- The macro runner does not include helpers from required modules in its
  synthetic compilation unit, despite syntax.md's module-helper rule. Backend
  selection therefore uses module resolution, with no user macro dependency.
- Derive's metadata tables use unqualified type names and prefer structs over
  enums. Combining unrelated same-named types can select the wrong shape.
  `FileAccess` and `OSError` avoid collisions in this integration; fieldless
  enums use their built-in equality. This does not repair derive in general.

These gaps were reported during implementation; the language spec was not
changed to accommodate them.

## Platform status

Native behavior is validated on Linux LP64. Darwin ABI values and backend
typing/emission are checked, but no macOS host or SDK was used to execute or
bootstrap the compiler. The Darwin directory binding is plain `readdir` (Apple Silicon has
no `$INODE64` alias; Intel macOS is not supported). Compiler target
initialisation for AArch64 is in docs/design/aarch64.md.
Complete macOS support also requires validating the linker, packaging scripts,
thread runtime, SIMD lowering, HTTP integration, and C-header conformance there.
macOS memory streams require the libc version that provides fmemopen (10.13+).

Paths and OS-produced text use UTF-8 strings. Byte-path support is future work;
OS-produced invalid UTF-8 currently follows str-from-bytes trap semantics.
Windows, other Unix systems, and 32-bit ABIs need their own backend work. The
current abstraction is a native LP64 Unix foundation, not a completed universal
OS layer. It leaves room for process spawning, signals, filesystem metadata,
DNS, polling backends, and platform capabilities as real consumers need them.

## Validation

Cases 7400–7408 cover backend identity and ABI differences; exclusive file
creation, binary I/O and byte limits; pipes, WouldBlock, EOF and closed
descriptors; environment, clocks, sleep, system info and entropy across the
256-byte boundary; TCP loopback; bounded streams and repeated close; runtime
adaptation; directories and executable paths; and rejected cross-thread stream
sharing. Accepted cases run with the native memory audit.

`scripts/test-os.sh` also compiles a C-header probe and compares every ABI field
plus the sizes and offsets used by shared FFI code. Its Darwin probe calls
every backend entry point during source compilation and emission, without
executing dummy resources. CI runs this script after the standard native cases.
`scripts/test-http.sh` verifies HTTP and TLS interoperability independently.
The full gate checks compiler bootstrap, the fixed point, and existing cases.

## Native contracts

Linux behavior follows the system headers and the Linux man-pages project's
[descriptor operations](https://man7.org/linux/man-pages/man2/open.2.html),
[directory reading](https://man7.org/linux/man-pages/man3/readdir.3.html), and
[memory streams](https://man7.org/linux/man-pages/man3/fmemopen.3.html).
Darwin contracts come from Apple's published
[fcntl](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/fcntl.h),
[socket](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/socket.h),
[errno](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/errno.h),
[directory](https://github.com/apple-oss-distributions/Libc/blob/main/include/dirent.h),
and [symbol-selection](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/cdefs.h)
headers, plus dyld's
[_NSGetExecutablePath](https://github.com/apple-oss-distributions/dyld/blob/main/dyld/DyldAPIs.cpp).
The implementation uses these public ABI contracts; it does not copy libc code.
