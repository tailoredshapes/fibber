//! The prelude's `Vec` built and read natively, for what the evaluator
//! makes itself: the items of `Form` values (quoted literals, macro
//! arguments) and `concat` (syntax §3.16). The layout is the prelude's
//! (lib/prelude.fib): `VecEmpty`, or `(VecOf cnt shift root tail)` with
//! a 32-way trie of `VLeaf`/`VBranch` nodes under `root` (`nil` while
//! everything fits in the tail) and the last 1..=32 elements in `tail`.

use crate::types::decls::ModuleId;
use crate::types::ty::{Scalar, TypeId};

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::object::Obj;
use super::value::Val;

const WIDTH: usize = 32;

impl Interp<'_> {
    fn vec_type(&self) -> R<TypeId> {
        self.p
            .globals
            .vec
            .ok_or_else(|| RunError::internal("the prelude defines no Vec"))
    }

    fn node_type(&self) -> R<TypeId> {
        self.p
            .globals
            .type_name(ModuleId::Prelude, "VNode")
            .ok_or_else(|| RunError::internal("the prelude defines no VNode"))
    }

    /// A new object of `ty` storing `fields`; with heap placement the
    /// fields' own counts (fresh objects) pass to it.
    pub fn node(&mut self, ty: TypeId, variant: usize, fields: Vec<Val>, at: Placement) -> R<Val> {
        let v = self.new_data(ty, Some(variant), fields.clone(), at)?;
        if at == Placement::Heap {
            let fresh: Vec<Val> = fields.into_iter().filter(|f| f.obj().is_some()).collect();
            self.give_back(&fresh)?;
        }
        Ok(v)
    }

    fn array_of(&mut self, items: &[Val], at: Placement) -> R<Val> {
        self.new_array(items.to_vec(), at)
    }

    /// A vector of `items` (each stored, and so retained, by an array).
    pub fn build_vec(&mut self, items: &[Val], at: Placement) -> R<Val> {
        let vec = self.vec_type()?;
        if items.is_empty() {
            return self.new_data(vec, Some(0), Vec::new(), at);
        }
        let n = items.len();
        let split = n - ((n - 1) % WIDTH + 1);
        let (trie, tail) = items.split_at(split);
        let tail = self.array_of(tail, at)?;
        let (root, shift) = self.build_trie(trie, at)?;
        let int = |x: usize| Val::Int(x as i64, Scalar::I64);
        self.node(vec, 1, vec![int(n), int(shift), root, tail], at)
    }

    /// The root (`nil` when empty) and shift of a trie of `items`, a
    /// multiple of 32 long.
    fn build_trie(&mut self, items: &[Val], at: Placement) -> R<(Val, usize)> {
        if items.is_empty() {
            return Ok((Val::None, 5));
        }
        let node = self.node_type()?;
        let mut nodes = Vec::new();
        for chunk in items.chunks(WIDTH) {
            let arr = self.array_of(chunk, at)?;
            nodes.push(self.node(node, 0, vec![arr], at)?);
        }
        let mut shift = 5;
        loop {
            let mut up = Vec::new();
            for chunk in nodes.chunks(WIDTH) {
                let arr = self.array_of(chunk, at)?;
                if at == Placement::Heap {
                    self.give_back(chunk)?;
                }
                up.push(self.node(node, 1, vec![arr], at)?);
            }
            if up.len() == 1 {
                return Ok((Val::Some(Box::new(up.remove(0))), shift));
            }
            nodes = up;
            shift += 5;
        }
    }

    /// The elements of a vector, in order (read on the heap).
    pub fn read_vec(&mut self, v: &Val) -> R<Vec<Val>> {
        let id = v.expect_obj("a vector")?;
        match self.objs.get(&self.heap, id)? {
            Obj::Variant { tag: 0, .. } => return Ok(Vec::new()),
            Obj::Variant { tag: 1, .. } => {}
            o => return Err(RunError::internal(format!("not a vector: {o:?}"))),
        }
        let root = self.field(id, 2)?;
        let tail = self.field(id, 3)?;
        let mut out = Vec::new();
        if let Val::Some(r) = root {
            self.read_node(&r, &mut out)?;
        }
        out.extend(self.read_array(&tail)?);
        Ok(out)
    }

    fn read_node(&mut self, node: &Val, out: &mut Vec<Val>) -> R<()> {
        let id = node.expect_obj("a vector node")?;
        let (tag, arr) = match self.objs.get(&self.heap, id)? {
            Obj::Variant { tag, .. } => (*tag, self.field(id, 0)?),
            o => return Err(RunError::internal(format!("not a vector node: {o:?}"))),
        };
        let items = self.read_array(&arr)?;
        if tag == 0 {
            out.extend(items);
            return Ok(());
        }
        for kid in &items {
            self.read_node(kid, out)?;
        }
        Ok(())
    }

    fn read_array(&mut self, a: &Val) -> R<Vec<Val>> {
        let id = a.expect_obj("an array")?;
        let n = match self.objs.get(&self.heap, id)? {
            Obj::Array(items) => items.len(),
            o => return Err(RunError::internal(format!("not an array: {o:?}"))),
        };
        (0..n).map(|i| self.field(id, i)).collect()
    }

    /// `(concat v..)`: a new vector of the elements of each, in order.
    pub fn concat_vals(&mut self, parts: &[Val]) -> R<Val> {
        let mut items = Vec::new();
        for p in parts {
            items.extend(self.read_vec(p)?);
        }
        self.build_vec(&items, Placement::Heap)
    }
}
