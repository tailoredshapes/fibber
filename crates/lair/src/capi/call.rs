//! Calling an address from C: `n <= 8` integer or pointer arguments,
//! dispatched by arity, the result an integer or a double.
//!
//! There is no error channel here (the signatures are §9's), so a call
//! that cannot be made is not made: it returns 0.

use std::mem::transmute;

use super::error::shield;

/// The most arguments a call takes.
pub(super) const MAX_ARGS: usize = 8;

/// Call `addr` as `extern "C" fn(i64 × args.len()) -> $ret`.
macro_rules! by_arity {
    ($addr:expr, $args:expr, $ret:ty) => {{
        let (addr, a): (usize, &[i64]) = ($addr, $args);
        match a.len() {
            0 => transmute::<usize, extern "C" fn() -> $ret>(addr)(),
            1 => transmute::<usize, extern "C" fn(i64) -> $ret>(addr)(a[0]),
            2 => transmute::<usize, extern "C" fn(i64, i64) -> $ret>(addr)(a[0], a[1]),
            3 => transmute::<usize, extern "C" fn(i64, i64, i64) -> $ret>(addr)(a[0], a[1], a[2]),
            4 => transmute::<usize, extern "C" fn(i64, i64, i64, i64) -> $ret>(addr)(
                a[0], a[1], a[2], a[3],
            ),
            5 => transmute::<usize, extern "C" fn(i64, i64, i64, i64, i64) -> $ret>(addr)(
                a[0], a[1], a[2], a[3], a[4],
            ),
            6 => transmute::<usize, extern "C" fn(i64, i64, i64, i64, i64, i64) -> $ret>(addr)(
                a[0], a[1], a[2], a[3], a[4], a[5],
            ),
            7 => {
                transmute::<usize, extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> $ret>(addr)(
                    a[0], a[1], a[2], a[3], a[4], a[5], a[6],
                )
            }
            _ => transmute::<usize, extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> $ret>(
                addr,
            )(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]),
        }
    }};
}

/// Call `addr` with `args`, reading the result as an integer.
///
/// # Safety
/// `addr` is a `ccc` function taking `args.len() <= 8` integer or
/// pointer parameters and returning an integer, a pointer or nothing;
/// `args.len() <= 8`.
pub(super) unsafe fn invoke(addr: usize, args: &[i64]) -> i64 {
    by_arity!(addr, args, i64)
}

/// As [`invoke`] for a function that returns a `double`.
///
/// # Safety
/// As [`invoke`], with a `double` result.
unsafe fn invoke_f64(addr: usize, args: &[i64]) -> f64 {
    by_arity!(addr, args, f64)
}

/// The arguments of a call, or `None` when it is not to be made: no
/// address, more than [`MAX_ARGS`] arguments, or a null `args` for
/// `n > 0`.
///
/// # Safety
/// `args` points to `n` readable `i64` when `n` is in 1..=8 and
/// `args` is not null.
unsafe fn arguments<'a>(addr: usize, args: *const i64, n: usize) -> Option<&'a [i64]> {
    if addr == 0 || n > MAX_ARGS || (n > 0 && args.is_null()) {
        return None;
    }
    Some(if n == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(args, n)
    })
}

/// Call the C-ABI function at `addr` with the `n <= 8` integer or
/// pointer arguments `args[0..n]` (each passed as a 64-bit integer; a
/// `float` or `double` parameter cannot be passed) on the calling
/// thread, and return its result as a 64-bit integer. A narrower
/// integer result has unspecified high bits; a pointer result is the
/// pointer's value; a `void` function's result is meaningless.
///
/// Returns 0 without calling when `addr` is 0, `n > 8`, or `args` is
/// null with `n > 0`.
///
/// # Safety
/// `addr` is 0 or the address of a `ccc` function whose parameters are
/// `n` integers or pointers, alive for the duration of the call (a
/// function of a [`LairJit`](super::LairJit) not yet freed); `args` is
/// null or points to `n` readable `int64_t`.
#[no_mangle]
pub unsafe extern "C" fn lair_call_i64(addr: usize, args: *const i64, n: usize) -> i64 {
    shield(0, || {
        #[cfg(feature = "test-panic")]
        if addr == super::error::TEST_PANIC_ADDRESS {
            panic!("injected by the test-panic feature");
        }
        arguments(addr, args, n).map_or(0, |a| invoke(addr, a))
    })
}

/// As [`lair_call_i64`] for a function that returns a `double`; 0.0
/// when the call is not made.
///
/// # Safety
/// As [`lair_call_i64`], the function returning `double`.
#[no_mangle]
pub unsafe extern "C" fn lair_call_f64(addr: usize, args: *const i64, n: usize) -> f64 {
    shield(0.0, || {
        #[cfg(feature = "test-panic")]
        if addr == super::error::TEST_PANIC_ADDRESS {
            panic!("injected by the test-panic feature");
        }
        arguments(addr, args, n).map_or(0.0, |a| invoke_f64(addr, a))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn sum3(a: i64, b: i64, c: i64) -> i64 {
        a * 100 + b * 10 + c
    }
    extern "C" fn half(a: i64) -> f64 {
        a as f64 / 2.0
    }

    #[test]
    fn arguments_that_cannot_make_a_call_make_none() {
        let args = [1i64; 9];
        let f = sum3 as extern "C" fn(i64, i64, i64) -> i64 as usize;
        // SAFETY: the pointers are real or null with n = 0 / refused before use.
        unsafe {
            assert_eq!(lair_call_i64(f, args.as_ptr(), 3), 111);
            assert_eq!(lair_call_i64(f, args.as_ptr(), 9), 0, "n > 8");
            assert_eq!(lair_call_i64(0, args.as_ptr(), 3), 0, "no address");
            assert_eq!(lair_call_i64(f, std::ptr::null(), 3), 0, "null args");
            assert_eq!(lair_call_f64(0, args.as_ptr(), 1), 0.0);
            let g = half as extern "C" fn(i64) -> f64 as usize;
            assert_eq!(lair_call_f64(g, [5i64].as_ptr(), 1), 2.5);
        }
    }
}
