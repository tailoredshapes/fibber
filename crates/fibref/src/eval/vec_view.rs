//! The pattern compiler's core view of the prelude's `Vec` (types
//! §8.3): the length from `cnt`, element `i` by one walk of the trie
//! (the path `vec-nth` takes) with no count operation, and the rest of
//! a vector pattern, a new vector of the elements from `k` on
//! (`fib.vec-drop`). The layout is the one `vecs.rs` builds: `VecEmpty`,
//! or `(VecOf cnt shift root tail)` over `VLeaf`/`VBranch` nodes.

use crate::heap::ObjId;

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::object::Obj;
use super::value::Val;

impl Interp<'_> {
    /// The vector's tag: `true` for `VecOf`.
    fn vec_nonempty(&self, id: ObjId) -> R<bool> {
        match self.objs.get(&self.heap, id)? {
            Obj::Variant { tag: 0, .. } => Ok(false),
            Obj::Variant { tag: 1, .. } => Ok(true),
            o => Err(RunError::internal(format!("not a vector: {o:?}"))),
        }
    }

    /// The length of the vector `v` (its tag, then `cnt`).
    pub fn vec_len(&mut self, v: &Val) -> R<usize> {
        let id = v.expect_obj("a vector pattern's scrutinee")?;
        if !self.vec_nonempty(id)? {
            return Ok(0);
        }
        let n = self.field(id, 0)?.as_int()?;
        usize::try_from(n).map_err(|_| RunError::internal("a vector with a negative count"))
    }

    /// Element `i < len` of the vector `v`, read without a count: a part
    /// of `v` (types §6.1).
    pub fn vec_elem(&mut self, v: &Val, i: usize) -> R<Val> {
        let id = v.expect_obj("a vector pattern's scrutinee")?;
        let cnt = self.vec_len(v)?;
        let tail = self.field(id, 3)?;
        let tail_id = tail.expect_obj("a vector's tail")?;
        let tail_len = self.fields(tail_id)?.len();
        let tailoff = cnt - tail_len;
        if i >= tailoff {
            return self.field(tail_id, i - tailoff);
        }
        let shift = self.field(id, 1)?.as_int()?;
        let Val::Some(root) = self.field(id, 2)? else {
            return Err(RunError::internal(
                "an element before the tail with no trie",
            ));
        };
        self.trie_elem(&root, shift, i)
    }

    /// `vnode-get`: from `node` at `level` down to the leaf holding `i`.
    fn trie_elem(&mut self, node: &Val, mut level: i64, i: usize) -> R<Val> {
        let mut node = node.clone();
        loop {
            let id = node.expect_obj("a vector node")?;
            let leaf = match self.objs.get(&self.heap, id)? {
                Obj::Variant { tag, .. } => *tag == 0,
                o => return Err(RunError::internal(format!("not a vector node: {o:?}"))),
            };
            let arr = self.field(id, 0)?.expect_obj("a vector node's array")?;
            if leaf {
                return self.field(arr, i & 31);
            }
            node = self.field(arr, (i >> level) & 31)?;
            level -= 5;
        }
    }

    /// `fib.vec-drop`: a new heap vector of the elements of `v` from `k`
    /// on, each stored (and so retained) by its array; the caller owns
    /// it.
    pub fn vec_drop(&mut self, v: &Val, k: usize) -> R<Val> {
        let items = self.read_vec(v)?;
        self.build_vec(&items[k.min(items.len())..], Placement::Heap)
    }
}
