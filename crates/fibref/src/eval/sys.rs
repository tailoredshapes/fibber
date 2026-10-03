//! The unix primitives (stdlib "sys primitives"): file descriptors as
//! `i64`, every failure the negated errno in the result and never a trap.
//! The compiled runtime does the same through the C library (`rt/sys.lir`);
//! the two are compared by `cases/stdlib` 3600 to 3649.
//!
//! This is the one module of the interpreter that calls the C library
//! (`libc`), so it is the one that allows `unsafe` code.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString};

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::value::Val;
use crate::types::ty::Scalar;

const EINVAL: i64 = 22;
/// The most a single read asks of the kernel: a larger `n` is clamped (a
/// read may be short), as `rt/sys.lir` clamps it.
const READ_MAX: i64 = 1 << 30;

/// The errno of the call that just failed.
fn errno() -> i64 {
    i64::from(std::io::Error::last_os_error().raw_os_error().unwrap_or(5))
}

/// `r` when the call succeeded, else the negated errno.
fn ret(r: i64) -> i64 {
    if r < 0 {
        -errno()
    } else {
        r
    }
}

fn cstr(s: &str) -> Option<CString> {
    CString::new(s).ok()
}

/// `(sys-open path flags mode)`.
pub fn open(path: &str, flags: i64, mode: i64) -> i64 {
    let Some(p) = cstr(path) else { return -EINVAL };
    // SAFETY: p is a NUL-terminated string that outlives the call.
    ret(i64::from(unsafe {
        libc::open(p.as_ptr(), flags as i32, mode as libc::c_uint)
    }))
}

/// `(sys-close fd)`: 0 or the negated errno.
pub fn close(fd: i64) -> i64 {
    // SAFETY: close takes any integer; a bad one is EBADF.
    ret(i64::from(unsafe { libc::close(fd as i32) }))
}

/// `(sys-read fd n)`: the status (bytes read, or the negated errno) and
/// then the bytes read.
pub fn read(fd: i64, n: i64) -> (i64, Vec<u8>) {
    if n < 0 {
        return (-EINVAL, vec![]);
    }
    let n = n.min(READ_MAX) as usize;
    let mut buf = vec![0u8; n];
    // SAFETY: buf has n writable bytes.
    let got = ret(unsafe { libc::read(fd as i32, buf.as_mut_ptr().cast(), n) } as i64);
    buf.truncate(got.max(0) as usize);
    (got, buf)
}

/// `(sys-write fd bytes off n)`.
pub fn write(fd: i64, bytes: &[u8], off: i64, n: i64) -> i64 {
    let len = bytes.len() as i64;
    if off < 0 || n < 0 || off > len || n > len - off {
        return -EINVAL;
    }
    let p = bytes[off as usize..].as_ptr();
    // SAFETY: n bytes from off lie inside bytes.
    ret(unsafe { libc::write(fd as i32, p.cast(), n as usize) } as i64)
}

pub fn seek(fd: i64, off: i64, whence: i64) -> i64 {
    // SAFETY: lseek takes integers only.
    ret(unsafe { libc::lseek(fd as i32, off, whence as i32) })
}

/// `(sys-pipe)`: read end shifted left 32 bits, or'd with the write end.
pub fn pipe() -> i64 {
    let mut fds = [0i32; 2];
    // SAFETY: fds has room for the two descriptors.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } < 0 {
        return -errno();
    }
    (i64::from(fds[0]) << 32) | i64::from(fds[1])
}

pub fn dup(fd: i64) -> i64 {
    // SAFETY: dup takes an integer only.
    ret(i64::from(unsafe { libc::dup(fd as i32) }))
}

pub fn isatty(fd: i64) -> bool {
    // SAFETY: isatty takes an integer only.
    unsafe { libc::isatty(fd as i32) == 1 }
}

pub fn unlink(path: &str) -> i64 {
    let Some(p) = cstr(path) else { return -EINVAL };
    // SAFETY: p is a NUL-terminated string that outlives the call.
    ret(i64::from(unsafe { libc::unlink(p.as_ptr()) }))
}

pub fn mkdir(path: &str, mode: i64) -> i64 {
    let Some(p) = cstr(path) else { return -EINVAL };
    // SAFETY: p is a NUL-terminated string that outlives the call.
    ret(i64::from(unsafe {
        libc::mkdir(p.as_ptr(), mode as libc::mode_t)
    }))
}

pub fn rmdir(path: &str) -> i64 {
    let Some(p) = cstr(path) else { return -EINVAL };
    // SAFETY: p is a NUL-terminated string that outlives the call.
    ret(i64::from(unsafe { libc::rmdir(p.as_ptr()) }))
}

/// strerror, as the C library words it.
pub fn errno_text(e: i64) -> String {
    // SAFETY: strerror returns a NUL-terminated string valid until the
    // next strerror call of this thread, and it is copied at once.
    let p = unsafe { libc::strerror(e as i32) };
    // SAFETY: p is non-null and NUL-terminated.
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// The environment variable, its bytes made UTF-8 as `from_utf8_lossy` does.
pub fn getenv(name: &str) -> Option<String> {
    let n = cstr(name)?;
    // SAFETY: n is a NUL-terminated string that outlives the call.
    let p = unsafe { libc::getenv(n.as_ptr()) };
    if p.is_null() {
        return None;
    }
    // SAFETY: p is non-null and NUL-terminated.
    Some(String::from_utf8_lossy(unsafe { CStr::from_ptr(p) }.to_bytes()).into_owned())
}

fn clock(id: libc::clockid_t) -> i64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: t is a valid timespec.
    unsafe { libc::clock_gettime(id, &mut t) };
    t.tv_sec * 1_000_000_000 + t.tv_nsec
}

pub fn clock_now() -> i64 {
    clock(libc::CLOCK_MONOTONIC)
}

pub fn wall_now() -> i64 {
    clock(libc::CLOCK_REALTIME)
}

/// Sleeps at least `ns` nanoseconds (not at all when `ns` is not positive),
/// resuming after a signal; returns 0.
pub fn sleep(ns: i64) -> i64 {
    if ns > 0 {
        std::thread::sleep(std::time::Duration::from_nanos(ns as u64));
    }
    0
}

fn byte(b: u8) -> Val {
    Val::Int(i64::from(b as i8), Scalar::I8)
}

impl Interp<'_> {
    /// The `sys-` builtins.
    pub fn sys_builtin(&mut self, name: &str, a: &[Val]) -> R<Val> {
        let arg = |i: usize| {
            a.get(i)
                .ok_or_else(|| RunError::internal(format!("{name} without argument {i}")))
        };
        let int = |i: usize| arg(i)?.as_int();
        let s = |i: usize| -> R<String> { Ok(self.string(arg(i)?)?.to_string()) };
        let i64v = |n: i64| Ok(Val::Int(n, Scalar::I64));
        match name {
            "sys-open" => i64v(open(&s(0)?, int(1)?, int(2)?)),
            "sys-close" => i64v(close(int(0)?)),
            "sys-read" => {
                let (status, bytes) = read(int(0)?, int(1)?);
                let mut items: Vec<Val> = status.to_le_bytes().iter().map(|b| byte(*b)).collect();
                items.extend(bytes.iter().map(|b| byte(*b)));
                self.new_array(items, Placement::Heap)
            }
            "sys-write" => {
                let bytes: Vec<u8> = self
                    .items(arg(1)?)?
                    .iter()
                    .map(|v| v.as_int().map(|n| n as u8))
                    .collect::<R<_>>()?;
                i64v(write(int(0)?, &bytes, int(2)?, int(3)?))
            }
            "sys-seek" => i64v(seek(int(0)?, int(1)?, int(2)?)),
            "sys-pipe" => i64v(pipe()),
            "sys-dup" => i64v(dup(int(0)?)),
            "sys-isatty" => Ok(Val::Bool(isatty(int(0)?))),
            "sys-unlink" => i64v(unlink(&s(0)?)),
            "sys-mkdir" => i64v(mkdir(&s(0)?, int(1)?)),
            "sys-rmdir" => i64v(rmdir(&s(0)?)),
            "sys-errno-text" => self.new_str(errno_text(int(0)?), Placement::Heap),
            "sys-getenv" => match getenv(&s(0)?) {
                Some(v) => {
                    let v = self.new_str(v, Placement::Heap)?;
                    Ok(Val::Some(Box::new(v)))
                }
                None => Ok(Val::None),
            },
            "sys-clock-now" => i64v(clock_now()),
            "sys-wall-now" => i64v(wall_now()),
            "sys-sleep" => i64v(sleep(int(0)?)),
            _ => Err(RunError::internal(format!("no sys builtin {name}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_the_negated_enoent() {
        assert_eq!(open("/nonexistent-fibber-dir/x", 0, 0), -2);
        assert_eq!(errno_text(2), "No such file or directory");
    }

    #[test]
    fn a_pipe_carries_bytes_and_ends() {
        let p = pipe();
        let (r, w) = (p >> 32, p & 0xffff_ffff);
        assert_eq!(write(w, b"abc", 0, 3), 3);
        assert_eq!(read(r, 10), (3, b"abc".to_vec()));
        assert_eq!(close(w), 0);
        assert_eq!(read(r, 10), (0, vec![]));
        assert_eq!(close(r), 0);
        assert_eq!(close(r), -9);
    }

    #[test]
    fn bad_arguments_are_einval_not_traps() {
        assert_eq!(read(0, -1).0, -22);
        assert_eq!(write(1, b"ab", 1, 5), -22);
        assert_eq!(open("a\0b", 0, 0), -22);
    }
}
