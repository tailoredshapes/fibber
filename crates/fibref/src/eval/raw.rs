//! `unsafe` memory (syntax §3.15, types §6.13): `alloc`, `free`, the
//! loads and stores, `ptr+`, `raw`, `raw-retained`, `release-raw`, and
//! the one `extern` the prelude declares (`write`; `strtod.rs` has the
//! others). Raw memory is a byte arena of its own: fibber objects are
//! never raw memory, and `raw` hands out a tagged address that only
//! `release-raw` reads back.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsFd;

use crate::heap::ObjId;

use super::error::{RunError, R};

/// Tag bit of an address that names a fibber object (`raw`).
const OBJECT_TAG: u64 = 1 << 63;

/// The arena: blocks from `alloc`, addressed as block and offset.
#[derive(Debug, Default)]
pub struct RawMemory {
    blocks: Vec<Option<Vec<u8>>>,
    /// The objects `raw` has given addresses to.
    objects: HashMap<u64, ObjId>,
    /// Duplicates of descriptors 1 and 2, made at the first `write` to
    /// each, so that a write is the system call and nothing buffers it.
    out: Option<File>,
    err: Option<File>,
}

impl RawMemory {
    /// `(alloc n)`: a fresh zeroed block.
    pub fn alloc(&mut self, n: i64) -> R<u64> {
        let n = usize::try_from(n).map_err(|_| RunError::trap(format!("alloc of {n} bytes")))?;
        self.blocks.push(Some(vec![0; n]));
        Ok((self.blocks.len() as u64) << 32)
    }

    /// `(free p)`.
    pub fn free(&mut self, p: u64) -> R<()> {
        let (b, off) = split(p)?;
        match self.blocks.get_mut(b) {
            Some(slot @ Some(_)) if off == 0 => {
                *slot = None;
                Ok(())
            }
            _ => Err(RunError::trap(format!("free of {p:#x}, not a live block"))),
        }
    }

    fn bytes(&mut self, p: u64, len: usize) -> R<&mut [u8]> {
        let (b, off) = split(p)?;
        let block = self
            .blocks
            .get_mut(b)
            .and_then(Option::as_mut)
            .ok_or_else(|| RunError::trap(format!("access to {p:#x}, not a live block")))?;
        let end = off
            .checked_add(len)
            .filter(|e| *e <= block.len())
            .ok_or_else(|| {
                RunError::trap(format!("access of {len} bytes at {p:#x} out of bounds"))
            })?;
        Ok(&mut block[off..end])
    }

    /// The bytes from `p` up to its terminating NUL, which must lie in
    /// the same block (a C string in raw memory).
    pub fn c_string(&mut self, p: u64) -> R<Vec<u8>> {
        let (b, off) = split(p)?;
        let block = self
            .blocks
            .get(b)
            .and_then(Option::as_ref)
            .ok_or_else(|| RunError::trap(format!("access to {p:#x}, not a live block")))?;
        let tail = block.get(off..).unwrap_or_default();
        match tail.iter().position(|c| *c == 0) {
            Some(n) => Ok(tail[..n].to_vec()),
            None => Err(RunError::trap(format!(
                "the string at {p:#x} has no NUL before the end of its block"
            ))),
        }
    }

    /// A little-endian load of `len` bytes, sign-extended.
    pub fn load(&mut self, p: u64, len: usize) -> R<i64> {
        let bytes = self.bytes(p, len)?;
        let mut buf = [0u8; 8];
        buf[..len].copy_from_slice(bytes);
        let raw = i64::from_le_bytes(buf);
        let shift = 64 - 8 * len as u32;
        Ok(if shift == 64 {
            raw
        } else {
            (raw << shift) >> shift
        })
    }

    /// A little-endian store of the low `len` bytes of `v`.
    pub fn store(&mut self, p: u64, len: usize, v: i64) -> R<()> {
        let bytes = self.bytes(p, len)?;
        bytes.copy_from_slice(&v.to_le_bytes()[..len]);
        Ok(())
    }

    /// The `extern write(fd, p, n)` of the prelude, on descriptor 1 or
    /// 2: one write(2) of the `n` bytes at `p`, giving the number of
    /// bytes written, which may be fewer than `n`, or -1 when the call
    /// failed (a full device, for one), as the C function does in
    /// compiled code.
    pub fn write(&mut self, fd: i64, p: u64, n: i64) -> R<i64> {
        let n = usize::try_from(n).map_err(|_| RunError::trap(format!("write of {n} bytes")))?;
        let bytes = self.bytes(p, n)?.to_vec();
        let slot = match fd {
            1 => &mut self.out,
            2 => &mut self.err,
            _ => return Err(RunError::unsupported(format!("write to fd {fd}"))),
        };
        if slot.is_none() {
            *slot = duplicate(fd).ok();
        }
        Ok(slot.as_mut().map_or(-1, |f| write_some(f, &bytes)))
    }
}

/// A duplicate of descriptor 1 or 2. The Rust runtime opens `/dev/null`
/// for a standard descriptor that is closed at start, so this fails only
/// when the process has no descriptor to spare, and then `write` is -1.
fn duplicate(fd: i64) -> io::Result<File> {
    let owned = if fd == 1 {
        io::stdout().as_fd().try_clone_to_owned()?
    } else {
        io::stderr().as_fd().try_clone_to_owned()?
    };
    Ok(File::from(owned))
}

/// One write to `sink`: the bytes it took, or -1.
fn write_some(sink: &mut impl Write, bytes: &[u8]) -> i64 {
    sink.write(bytes).map_or(-1, |n| n as i64)
}

fn split(p: u64) -> R<(usize, usize)> {
    let block = (p >> 32) as usize;
    if p & OBJECT_TAG != 0 || block == 0 {
        return Err(RunError::trap(format!(
            "{p:#x} is not an address of raw memory"
        )));
    }
    Ok((block - 1, (p & 0xffff_ffff) as usize))
}

impl RawMemory {
    /// The address `raw` gives the object `id`.
    pub fn object_address(&mut self, id: ObjId) -> u64 {
        let p = OBJECT_TAG | id.index() as u64;
        self.objects.insert(p, id);
        p
    }

    /// The object behind an address from `raw`, if it is one.
    pub fn address_object(&self, p: u64) -> Option<ObjId> {
        self.objects.get(&p).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_loads_round_trip_and_sign_extend() {
        let mut m = RawMemory::default();
        let p = m.alloc(8).expect("alloc");
        m.store(p + 1, 1, -1).expect("store");
        assert_eq!(m.load(p + 1, 1), Ok(-1));
        m.store(p, 8, 1 << 40).expect("store");
        assert_eq!(m.load(p, 8), Ok(1 << 40));
        assert!(m.load(p + 8, 1).is_err());
        m.free(p).expect("free");
        assert!(m.free(p).is_err());
        assert!(m.load(p, 1).is_err());
    }

    /// A sink that takes at most `.0` bytes of each write.
    struct Short(usize);

    impl Write for Short {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len().min(self.0))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_write_gives_the_bytes_taken_possibly_fewer_than_given() {
        assert_eq!(write_some(&mut Short(3), b"hello"), 3);
        assert_eq!(write_some(&mut Short(100), b"hello"), 5);
        assert_eq!(write_some(&mut Short(3), b""), 0);
    }

    #[test]
    fn a_failed_write_is_minus_one() {
        let mut full = File::options()
            .write(true)
            .open("/dev/full")
            .expect("open /dev/full");
        assert_eq!(write_some(&mut full, b"x"), -1);
        let mut null = File::options()
            .write(true)
            .open("/dev/null")
            .expect("open /dev/null");
        assert_eq!(write_some(&mut null, b"xyz"), 3);
    }

    #[test]
    fn a_write_to_another_descriptor_is_unsupported() {
        let mut m = RawMemory::default();
        let p = m.alloc(1).expect("alloc");
        assert!(m.write(3, p, 1).is_err());
        assert!(m.write(1, p, -1).is_err());
    }

    #[test]
    fn object_addresses_are_not_raw_memory() {
        let mut m = RawMemory::default();
        let id = crate::heap::Heap::new()
            .alloc(crate::heap::Kind::Immutable, vec![])
            .expect("alloc");
        let a = m.object_address(id);
        assert_eq!(m.address_object(a), Some(id));
        assert!(m.load(a, 1).is_err());
    }
}
