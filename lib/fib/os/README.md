# OS services

`fib.os` provides named OS operations, typed options, and shared errors. It
includes files, descriptors, directories, clocks, environment variables, process
identity, executable discovery, system information, and secure random bytes.
`fib.os.net`, `fib.os.platform`, and `fib.os.memory-stream` are explicit modules.

The native Linux implementation is exercised by the test suite. A Darwin backend
and runtime adaptation are included and checked for source typing and emission;
macOS linking, execution, and compiler bootstrap still need validation on a Mac.
The Darwin directory binding is plain `readdir`: on Apple Silicon there is no `$INODE64` alias (the 64-bit-inode layout is the only one), and Intel macOS is not a target.
Apple Silicon also needs its libc symbol selection and compiler target setup.
These bindings assume LP64; Windows and 32-bit platforms have no backend yet.

## Errors and options

Operations that can fail return `(Result T os/OSError)`. An `OSError` contains
`kind`, `code`, `operation`, and `message`. Branch on `kind`; the native numeric
`code` is retained for diagnostics and differs between platforms. Kinds include
`NotFound`, `PermissionDenied`, `AlreadyExists`, `InvalidInput`, `WouldBlock`,
`Interrupted`, `TimedOut`, `BadDescriptor`, `BrokenPipe`, connection errors,
`NotSupported`, `LimitExceeded`, `Cancelled`, and `Other`.

File flags are represented by `FileOptions`, with `FileAccess` variants
`ReadOnly`, `WriteOnly`, and `ReadWrite`. Defaults are read-only, mode 0644, and
no creation, truncation, or append. Opening uses close-on-exec. `create-new`
provides exclusive creation; permissions remain subject to the process umask.

```clojure
(ns main (:require [fib.os :as os]))

(defun main () -> i64
  (match (os/open-file "/tmp/example.dat"
    (with (os/default-file-options)
      (access os/WriteOnly) (create-new true) (mode 384)))
    ((Err error) (do (eprintln (. error message)) 1))
    ((Ok fd)
      (let [written (os/write-all fd (str-bytes "hello\n"))
            closed (os/close-fd fd)]
        (match (try-let [_ written _ closed] (Ok ()))
          ((Ok _) 0) ((Err error) (do (eprintln (. error message)) 1)))))))
```

Descriptors are explicit resources. Close each owned descriptor on every
ordinary exit, including Result errors; the language does not attach an OS
destructor to an integer. Do not close it twice or use it after close: the OS
can reuse its numeric value. `duplicate-fd` follows POSIX `dup` semantics and
creates an inheritable descriptor; use `set-inheritable` when needed.

## Services

| Module | Public operations |
|---|---|
| `fib.os.files` | `open-file`, `read-file-bytes path limit`, `write-file-bytes`, `remove-file`, `create-directory`, `remove-directory` |
| `fib.os.io` | `read-fd fd size`, `write-fd`, `write-fd-range`, `write-all`, `close-fd`, `seek-fd`, `duplicate-fd`, `make-pipe`, `terminal?`, `set-nonblocking`, `set-inheritable`, standard descriptors |
| `fib.os.directories` | `list-directory`, `create-temp-directory template` |
| `fib.os.time` | checked `monotonic-ns`, `wall-ns`, `sleep-ns`; convenience `clock-now`, `wall-now` |
| `fib.os.environment` | `getenv`, `setenv`, `unsetenv`, `temp-directory` |
| `fib.os.process` | `process-id`, `parent-process-id`, `executable-path`, `current-directory`, `change-directory`, `thread-count`, `fork-run` and `fork-start`, `wait-any-child`, `wait-child`, `fork-collect` (a closure run in a child process; refused while other threads exist) |
| `fib.os.system` | `system-info`, `page-size`, `available-cpus` |
| `fib.os.random` | `random-bytes size` |
| `fib.os.platform` | `current`, `name`, `from-name`; `Platform` variants `Linux`, `Darwin`, `Unsupported` |

Bytes remain `(Array i8)`, including embedded NULs. `read-fd` may return fewer
bytes than requested; an empty array means EOF, while a nonblocking empty pipe
returns `WouldBlock`. `write-fd` may write a prefix; `write-all` handles partial
writes and interrupts. It can return `WouldBlock` for a nonblocking descriptor.
`seek-fd` takes `Start`, `Current`, or `End`. Pipes are close-on-exec and initially
blocking. Paths and environment text use Fibber UTF-8 strings; NULs are rejected.
OS-provided names currently must also be valid UTF-8.

`list-directory` omits `.` and `..` and returns unsorted `DirectoryEntry` values
with `name` and `kind`. Kinds are `RegularFile`, `Directory`, `SymbolicLink`,
`UnknownEntry`, and `OtherEntry`. A filesystem can report an unknown kind;
applications must not treat that as proof that an entry is a regular file.
Temporary directory templates must end in `XXXXXX`; creation uses mode 0700.

Clocks and sleep durations are nanoseconds. Monotonic time is for deadlines;
wall time is measured from the Unix epoch. Sleep resumes after interrupts.
The convenience clocks preserve the existing primitive API; use checked clocks
when clock failure must be handled explicitly. SystemInfo contains system,
hostname, release, version, and machine strings. Random bytes come from
`getentropy`, in requests of at most 256 bytes, with a 64 MiB total limit.

Environment variables and the working directory belong to the process. Their
mutations affect every thread; arrange them before starting workers or
synchronize callers. `getenv` returns an Option, including nil for invalid
names. `temp-directory` uses `TMPDIR`, falling back to `/tmp`.

## TCP and polling

```clojure
(:require [fib.os.net :as net] [fib.os.io :as io])
(net/listen-tcp "127.0.0.1" 0 128)
(net/connect-tcp "127.0.0.1" port 1000)
```

`listen-tcp host port backlog` returns `Listener(fd port)`; zero selects an
ephemeral port. `accept-tcp listener-fd` returns `Accepted(fd peer port)` or
`WouldBlock`. `connect-tcp host port timeout-ms` returns a connected descriptor.
Hosts are numeric IPv4 or IPv6 addresses; DNS resolution is future work.
All sockets created or accepted here are nonblocking and close-on-exec.

`ready? fd writing? timeout-ms` polls for read or write readiness. False means
timeout; a ready descriptor can still report EOF or an I/O error on the next
operation. `wait-readable fd deadline-ns stop-atom` waits with a monotonic
deadline and cancellation. `send-all fd bytes timeout-ms stop-atom` handles
partial writes, interrupts, backpressure, cancellation, and SIGPIPE suppression.
Use it with a nonblocking socket, as returned by this module. Close all owned
socket descriptors with `io/close-fd`. TCP operation timeouts are bounded to
one hour; cancellation is checked at poll intervals of at most 100 ms.

## Native memory streams

`fib.os.memory-stream/open-stream limit` returns a bounded writable FILE-backed
`Stream`. `write-bytes`, `bytes`, `full?`, and `close-stream` manage it; close is
idempotent, access after close returns `BadDescriptor`, and exceeding the limit
returns `LimitExceeded`. Streams are local capabilities; the ownership checker
rejects sending them to another thread. Limits range from zero to 64 MiB.

`native-handle` is an unsafe-interoperability escape hatch: its pointer is valid
only while the stream is open. Foreign code must respect that lifetime. HTTP
uses these streams as libcurl write destinations. Buffers include one extra
byte for stdio's terminator and use the actual written length to preserve NULs.

## Validation and extension

```sh
OS_FIBC=./F scripts/test-os.sh
```

This runs native behavior and ownership cases, compares ABI values and layouts
against compiled C headers, and emits the Darwin backend. It does not emulate
macOS. See [the design record](../../../docs/design/os.md) for the backend
contract, compiler integration, platform limits, and extension points.
