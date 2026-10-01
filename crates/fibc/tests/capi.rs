//! The C interface to lair (spec/compiler.md §9) from fibber: the
//! binding modules of `compiler/lair/` (`lair.ffi`, `lair.jit`,
//! `lair.call`, `lair.fibm`, `lair.expand`) and the program that uses
//! them, `compiler/jit-demo.fib`, built with `fibc build -L DIR -l lair`
//! against the `liblair.so` that `cargo build -p lair` makes, and run
//! without `LD_LIBRARY_PATH`.
//!
//! - `basic.rs`: a session, the checker, executables and a hook round trip
//!   with a hand-written lIR module, each line compared with the Rust API.
//! - `macros.rs`: real macro-time modules, as `fibc` makes them, run from
//!   fibber (arguments become `Form` objects, the hooks are served from
//!   fibber, the result is printed) and compared with what the Rust
//!   runner returns for the same macro.
//! - `stage1.rs`: a bug of the compiler that these modules had to work
//!   around (a struct that holds a raw `ptr` released the block on drop),
//!   kept as a regression test now that it is fixed.

#![cfg(unix)]

#[path = "capi/basic.rs"]
mod basic;
#[path = "../../lair/tests/common/bounded.rs"]
mod bounded;
#[path = "capi/macros.rs"]
mod macros;
#[path = "capi/recording.rs"]
mod recording;
#[path = "capi/stage1.rs"]
mod stage1;
#[path = "capi/support.rs"]
mod support;
