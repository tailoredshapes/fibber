/*
 * lair.h -- the C interface to lair (liblair.so), spec/compiler.md section 9.
 *
 * lair compiles lIR (spec/lir.md) to native code through LLVM, in
 * process (a JIT session) or ahead of time (an executable). This is the
 * interface the self-hosted fibber compiler uses; it is the same two
 * operations fibc's own Rust runner performs, nothing more.
 *
 * Conventions
 *
 *  - A string is a pointer and a byte length: UTF-8, not NUL terminated.
 *    A null pointer is accepted only with length 0 (the empty string).
 *  - A function that can fail returns a lair_error * (NULL on success)
 *    and gives its value through an out-parameter, which it leaves
 *    untouched on failure (lair_jit_new sets it to NULL). The caller
 *    owns the error: read it with lair_error_text, free it with
 *    lair_error_free. The text is what lair itself would print, a
 *    diagnostic per line.
 *  - A null argument the callee needs is an error, never undefined
 *    behaviour. A pointer that is not null but wrong cannot be seen and
 *    is the caller's undefined behaviour.
 *  - No panic crosses this boundary: an internal failure is an error
 *    (or -1, or NULL) whose text starts "internal error:". The handle
 *    that was in use is then suspect and may only be freed.
 *  - Nothing is global. A handle may be used from one thread at a time
 *    (any thread); different handles may be used from different
 *    threads at once. Handles are heap objects; they do not depend on
 *    the thread that made them.
 *  - Memory: a handle is freed by its own free function; an address
 *    lair gives out is owned by the JIT session it came from and dies
 *    with lair_jit_free.
 */
#ifndef LAIR_H
#define LAIR_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ */
/* Errors                                                             */
/* ------------------------------------------------------------------ */

/* An error: opaque; it owns its message. */
typedef struct lair_error lair_error;

/* The message of `e`, NUL terminated; its length in bytes (without the
 * NUL) goes to *len unless len is NULL. A NULL `e` (success) gives "".
 * The text is valid until `e` is freed; do not modify it. */
const char *lair_error_text(const lair_error *e, size_t *len);

/* Free an error. NULL does nothing. */
void lair_error_free(lair_error *e);

/* ------------------------------------------------------------------ */
/* JIT sessions                                                       */
/* ------------------------------------------------------------------ */

/* A JIT session: modules added to it see each other's functions
 * through `declare` (spec/lir.md section 11). Opaque. */
typedef struct lair_jit lair_jit;

/* A new session. opt_level is 0 (no IR optimisation) to 3. On success
 * *out is the session; on failure *out is NULL (when `out` is not). */
lair_error *lair_jit_new(int opt_level, lair_jit **out);

/* End a session. Every address it gave out dies with it: calling one
 * afterwards, or while the call is in progress, is undefined behaviour.
 * NULL does nothing. */
void lair_jit_free(lair_jit *jit);

/* Parse, check, lower, verify and add the module `src` under the module
 * name `name` (the name a later "duplicate definition" message cites).
 * On failure nothing of the module was added, the error text is
 * what the Rust function Jit::add_source reports, and the session stays
 * usable. */
lair_error *lair_jit_add_source(lair_jit *jit, const char *name, size_t name_len,
                                const char *src, size_t src_len);

/* The address of the exported function `name`, compiled if need be. A
 * function that is not `ccc` is at an address of its own convention:
 * use lair_jit_c_entry to call it from C. */
lair_error *lair_jit_address(lair_jit *jit, const char *name, size_t name_len, size_t *out);

/* A `ccc` entry to the function `name`: the function itself when it is
 * `ccc`, else a trampoline the session generates once. Valid until the
 * session is freed. */
lair_error *lair_jit_c_entry(lair_jit *jit, const char *name, size_t name_len, size_t *out);

/* ------------------------------------------------------------------ */
/* Checking, and ahead-of-time compilation                            */
/* ------------------------------------------------------------------ */

/* Parse and check the lIR module `src` as a whole; no code is made.
 * NULL when valid, else the diagnostics (as `lair check` prints them,
 * without a file name). */
lair_error *lair_check_source(const char *src, size_t src_len);

/* Compile the module `src`, which must satisfy the `main` rule (spec/lir.md
 * section 7.2), and link it with `cc` into the executable `path`.
 * opt_level is 0 to 3. Each of the n_libs names libs[i] (of length
 * lib_lens[i]) is linked as -lNAME; libs and lib_lens may be NULL when
 * n_libs is 0. The executable's exit status is main's result. */
lair_error *lair_build_executable(const char *src, size_t src_len, const char *path,
                                  size_t path_len, int opt_level, const char *const *libs,
                                  const size_t *lib_lens, size_t n_libs);

/* ------------------------------------------------------------------ */
/* Calling an address                                                 */
/* ------------------------------------------------------------------ */

/* Call the C-ABI function at `addr` on the calling thread with the
 * n <= 8 integer or pointer arguments args[0..n), each passed as a
 * 64-bit integer, and return its result as a 64-bit integer. A
 * float or double *parameter* cannot be passed. A narrower integer
 * result has unspecified high bits; a pointer result is the pointer's
 * value; a void function's result is meaningless. The call is on the
 * caller's stack: use a lair_call (below) for a deep one.
 *
 * Returns 0 WITHOUT calling when addr is 0, n > 8, or args is NULL with
 * n > 0: these signatures have no error channel, so check them first. */
int64_t lair_call_i64(size_t addr, const int64_t *args, size_t n);

/* As lair_call_i64 for a function that returns a double (0.0 when the
 * call is not made). */
double lair_call_f64(size_t addr, const int64_t *args, size_t n);

/* ------------------------------------------------------------------ */
/* A call the compiler answers: the mailbox and the hooks             */
/* ------------------------------------------------------------------ */

/* A macro-time module calls back into the compiler for gensym and the
 * reflection builtins through two hook pointers and a context pointer
 * that the compiler installs in the module (fibm.set-hooks). Fibber has
 * no way to give C a pointer to one of its own functions, so lair
 * supplies the hooks, and the context pointer is a mailbox:
 *
 *   1. lair_call_new() makes a mailbox `c`.
 *   2. The compiler installs lair_hook1_address() (and/or
 *      lair_hook2_address()) and `c` as the context, through the module.
 *   3. lair_call_start(c, entry, args, n) runs the macro on a worker
 *      thread of lair with a 64 MiB stack.
 *   4. loop: k = lair_call_wait(c);
 *        k == 0: the call returned; lair_call_result(c) is its value.
 *        k == 1 or 2: the worker is parked in a hook call with k
 *          arguments. Read them with lair_call_hook_arg(c, 0 .. k-1),
 *          build the answer (with the module's own functions), and
 *          lair_call_hook_reply(c, answer): the worker resumes and the
 *          hook returns the answer. Then wait again.
 *        k == -1: a fault (see below).
 *   5. lair_call_free(c).
 *
 * The worker runs from lair_call_start, and again from each
 * lair_call_hook_reply, until its next hook call or its return. The
 * compiler's thread may touch the module's objects only after
 * lair_call_wait has returned and before the next lair_call_hook_reply
 * or lair_call_start. Within that rule the two never run on the module's
 * data at once, and each hands the other the mailbox's lock, so a
 * module's non-atomic data is safe. The rule is the compiler's to keep:
 * nothing here can see it broken.
 * A trap in the module aborts the process, as every trap does.
 *
 * Out-of-order use is defined. The misused function does nothing and
 * returns its failure value (a void one cannot); the mailbox records
 * the text of the first such "fault" (lair_call_fault) until the next
 * accepted lair_call_start, and the next lair_call_wait returns -1,
 * once. A call that was running is left as it was: wait again and it is
 * collected as usual.
 *
 *   start while a call is running / parked at a hook / not yet
 *     resumed: refused. start with addr 0, n > 8, or NULL args with
 *     n > 0: refused.
 *   wait with no call ever started: -1. wait on a call that is done:
 *     0 again; on one parked at a hook: its arity again.
 *   hook_arg with no hook waiting, or i not below the arity: 0.
 *   hook_reply with no hook waiting (nothing pending, or already
 *     answered): ignored.
 *   result before the call is done: 0.
 *   a hook called by a thread other than the call's worker, or when no
 *     call is running: returns 0 at once (it must not wait).
 *   free with the call running or parked: the mailbox is detached. The
 *     worker holds its own reference to the shared state, so nothing it
 *     uses is freed, but it is not stopped (it cannot be, safely): it
 *     finishes and its result is dropped, or, parked, stays parked
 *     forever; at its next hook call it parks forever too. The thread
 *     and a few words stay allocated until the process ends. A detached
 *     worker may still be running the session's code, so the session
 *     must not be freed (lair_jit_free) while one might: finish the call
 *     first.
 *   a NULL mailbox: start, hook_reply and free do nothing; wait is -1;
 *     hook_arg and result are 0.
 */
typedef struct lair_call lair_call;

/* A new, idle mailbox; NULL if it cannot be made. */
lair_call *lair_call_new(void);

/* Free a mailbox; NULL does nothing. See "free" above for a call that
 * has not finished. */
void lair_call_free(lair_call *c);

/* The hook pointers: `(cx, a) -> word` and `(cx, a, b) -> word`, where
 * each of cx, a, b and the result is one machine word (an integer or a
 * pointer). Install one in the module with the mailbox as `cx`;
 * calling it from the module's code, on the worker, parks the worker
 * as above. */
size_t lair_hook1_address(void);
size_t lair_hook2_address(void);

/* Run addr(args[0..n)) (a `ccc` function of n <= 8 integer or pointer
 * arguments; the arguments are copied) on a worker thread of this
 * mailbox, and return at once. A mailbox runs one call at a time and
 * may be reused after one finished (its result is then dropped).
 * `addr` must stay alive until the call has finished. */
void lair_call_start(lair_call *c, size_t addr, const int64_t *args, size_t n);

/* Block until the call returned (0) or is parked at a hook call (its
 * arity, 1 or 2). -1, once, after one or more misuses (see above), and
 * for a NULL mailbox or one with no call started. */
int lair_call_wait(lair_call *c);

/* Argument i (0 or 1) of the hook call the worker is parked at. */
int64_t lair_call_hook_arg(lair_call *c, size_t i);

/* Answer the hook call the worker is parked at with `v`; the worker
 * resumes and the hook returns `v`. */
void lair_call_hook_reply(lair_call *c, int64_t v);

/* The value of the finished call, read as lair_call_i64 reads one. */
int64_t lair_call_result(lair_call *c);

/* The text of the first misuse of this mailbox since the last accepted
 * lair_call_start: NUL terminated; its length without the NUL goes to
 * *len unless len is NULL. NULL (and *len 0) if there is none; a NULL
 * mailbox has the fault "the mailbox is null". The text lives until the
 * next accepted start or the free. */
const char *lair_call_fault(lair_call *c, size_t *len);

#ifdef __cplusplus
}
#endif

#endif /* LAIR_H */
