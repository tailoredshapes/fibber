//! The C interface to `lair` (spec/compiler.md §9, `include/lair.h`).
//!
//! The self-hosted compiler is a fibber program; it reaches `lair`
//! through `extern`, so every operation `fibc`'s own Rust runner does
//! through [`crate::Jit`] and [`crate::aot`] is also a C function here,
//! built on those same two and nothing else: the two consumers cannot
//! drift apart. `liblair.so` is this crate as a `cdylib`.
//!
//! Conventions, the same for every function:
//!
//! - A string is a pointer and a byte length (UTF-8, not NUL
//!   terminated). A null pointer is accepted only with length 0.
//! - A function that can fail returns a `lair_error *` (null on
//!   success) and gives its value through an out-parameter, which it
//!   leaves untouched on failure unless said otherwise. The error is
//!   the caller's; [`lair_error_text`] reads it, [`lair_error_free`]
//!   frees it.
//! - A null argument that the callee needs is an error, never
//!   undefined behaviour. A pointer that is not null but wrong is the
//!   caller's undefined behaviour, which no check can see.
//! - No panic crosses the boundary: each entry catches it and reports
//!   it as an error (or `-1`, or a null) whose text starts `internal
//!   error:`. A handle whose operation panicked may only be freed.
//! - `lair` keeps no state of its own outside the handles: the one
//!   process-wide thing is LLVM's native target registry, which
//!   `llvm::target::init` initialises once, on first use, behind a
//!   `std::sync::Once`. Handles are plain heap objects; a handle may
//!   be used from one thread at a time, any thread, and different
//!   handles from different threads at once.
//!
//! The functions are in `jit.rs` (a session), `check.rs`, `aot.rs`,
//! `error.rs`, `call.rs` (calling an address) and `mailbox.rs` (a call
//! that can be answered, `exchange.rs` being its state machine).

mod aot;
mod call;
mod check;
mod error;
mod exchange;
#[cfg(test)]
mod header;
mod jit;
mod mailbox;

pub use aot::lair_build_executable;
pub use call::{lair_call_f64, lair_call_i64};
pub use check::lair_check_source;
pub use error::{lair_error_free, lair_error_text, LairError};
#[cfg(feature = "test-panic")]
pub use error::{TEST_PANIC_ADDRESS, TEST_PANIC_SOURCE};
pub use jit::{
    lair_jit_add_source, lair_jit_address, lair_jit_c_entry, lair_jit_free, lair_jit_new, LairJit,
};
pub use mailbox::{
    lair_call_fault, lair_call_free, lair_call_hook_arg, lair_call_hook_reply, lair_call_new,
    lair_call_result, lair_call_start, lair_call_wait, lair_hook1_address, lair_hook2_address,
    Call,
};
