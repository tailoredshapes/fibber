//! `unsafe` memory (syntax §3.15, types §6.13): `alloc`, `free`, the
//! loads and stores, `ptr+`, `raw`, `raw-retained`, `release-raw`, and
//! the one `extern` the prelude declares (`write`). Raw memory is a
//! byte arena of its own: fibber objects are never raw memory, and
//! `raw` hands out a tagged address that only `release-raw` reads back.

use std::collections::HashMap;
use std::io::Write;

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

    /// The `extern write(fd, p, n)` of the prelude: fd 1 and 2 only.
    pub fn write(&mut self, fd: i64, p: u64, n: i64) -> R<i64> {
        let n = usize::try_from(n).map_err(|_| RunError::trap(format!("write of {n} bytes")))?;
        let bytes = self.bytes(p, n)?.to_vec();
        let written = match fd {
            1 => std::io::stdout().write_all(&bytes),
            2 => std::io::stderr().write_all(&bytes),
            _ => return Err(RunError::unsupported(format!("write to fd {fd}"))),
        };
        written.map_err(|e| RunError::trap(format!("write failed: {e}")))?;
        Ok(n as i64)
    }
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
