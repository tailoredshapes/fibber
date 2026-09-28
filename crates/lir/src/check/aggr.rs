//! Struct literals, vector operations and aggregates (spec/lir.md §3,
//! §6.4).

use super::arith::mask;
use super::fcx::Fcx;
use crate::ast::{Expr, Kind};
use crate::diag::{err, Pos, Result};
use crate::types::Type;

impl<'a> Fcx<'a> {
    pub fn struct_literal(
        &mut self,
        name: &Option<String>,
        fields: &'a [Expr],
        p: Pos,
    ) -> Result<Type> {
        let Some(n) = name else {
            let tys = fields.iter().map(|f| self.val(f)).collect::<Result<_>>()?;
            return Ok(Type::Anon(tys));
        };
        let ty = Type::Named(n.clone());
        self.valid(&ty, p)?;
        let want = self.env.structs[n].clone();
        if want.len() != fields.len() {
            return err(
                p,
                format!(
                    "{ty} literal: {} fields expected, found {}",
                    want.len(),
                    fields.len()
                ),
            );
        }
        for (i, (f, w)) in fields.iter().zip(&want).enumerate() {
            let got = self.val(f)?;
            if got != *w {
                return err(
                    f.pos,
                    format!("{ty} literal: field {} has type {got}, expected {w}", i + 1),
                );
            }
        }
        Ok(ty)
    }

    fn vector_of(&mut self, op: &str, v: &'a Expr, p: Pos) -> Result<(u32, Type)> {
        match self.val(v)? {
            Type::Vector(n, e) => Ok((n, *e)),
            t => err(p, format!("{op} needs a vector, found {t}")),
        }
    }

    fn lane_index(&mut self, op: &str, i: &'a Expr, n: u32, vt: &Type, p: Pos) -> Result<()> {
        let ti = self.val(i)?;
        if !ti.is_int() {
            return err(p, format!("{op} index must be an integer, found {ti}"));
        }
        if let Kind::Int(Type::Int(b), v) = &i.kind {
            let u = (*v as u128) & mask(*b);
            if u >= u128::from(n) {
                return err(p, format!("vector index {u} out of range for {vt}"));
            }
        }
        Ok(())
    }

    pub fn vector_op(&mut self, e: &'a Expr) -> Result<Type> {
        let p = e.pos;
        match &e.kind {
            Kind::ExtractElement(v, i) => {
                let (n, el) = self.vector_of("extractelement", v, p)?;
                self.lane_index(
                    "extractelement",
                    i,
                    n,
                    &Type::Vector(n, Box::new(el.clone())),
                    p,
                )?;
                Ok(el)
            }
            Kind::InsertElement(v, x, i) => {
                let (n, el) = self.vector_of("insertelement", v, p)?;
                let vt = Type::Vector(n, Box::new(el.clone()));
                let tx = self.val(x)?;
                self.same("insertelement", 2, &tx, &el, p)?;
                self.lane_index("insertelement", i, n, &vt, p)?;
                Ok(vt)
            }
            Kind::Shuffle(a, b, m) => self.shuffle(a, b, m, p),
            _ => err(p, "not a vector operation"),
        }
    }

    fn shuffle(&mut self, a: &'a Expr, b: &'a Expr, m: &'a Expr, p: Pos) -> Result<Type> {
        let (n, el) = self.vector_of("shufflevector", a, p)?;
        let vt = Type::Vector(n, Box::new(el.clone()));
        let tb = self.val(b)?;
        self.same("shufflevector", 2, &tb, &vt, p)?;
        let lanes = match &m.kind {
            Kind::Vector(Type::Vector(_, e), lanes) if **e == Type::Int(32) => lanes,
            _ => return err(p, "shufflevector mask must be a literal <M x i32>"),
        };
        for l in lanes {
            match l.int_literal() {
                Some(v) if v >= 0 && v < 2 * i128::from(n) => {}
                Some(v) => return err(p, format!("shufflevector mask element {v} out of range")),
                None => return err(p, "shufflevector mask must be a literal <M x i32>"),
            }
        }
        Ok(Type::Vector(lanes.len() as u32, Box::new(el)))
    }

    /// The type reached by `idx` from struct type `t`.
    fn field_path(&self, op: &str, t: &Type, idx: &[i128], p: Pos) -> Result<Type> {
        let mut cur = t.clone();
        for &i in idx {
            let Some(fields) = self.env.fields(&cur) else {
                return err(p, format!("{op}: cannot index into {cur}"));
            };
            if i < 0 || i as usize >= fields.len() || i > u32::MAX as i128 {
                return err(p, format!("{op}: field index {i} out of range for {cur}"));
            }
            cur = fields[i as usize].clone();
        }
        Ok(cur)
    }

    pub fn extract_value(&mut self, a: &'a Expr, idx: &[i128], p: Pos) -> Result<Type> {
        let ta = self.val(a)?;
        if !ta.is_aggregate() {
            return err(p, format!("extractvalue needs a struct, found {ta}"));
        }
        self.field_path("extractvalue", &ta, idx, p)
    }

    pub fn insert_value(&mut self, a: &'a Expr, v: &'a Expr, idx: &[i128], p: Pos) -> Result<Type> {
        let ta = self.val(a)?;
        if !ta.is_aggregate() {
            return err(p, format!("insertvalue needs a struct, found {ta}"));
        }
        let want = self.field_path("insertvalue", &ta, idx, p)?;
        let tv = self.val(v)?;
        self.same("insertvalue", 2, &tv, &want, p)?;
        Ok(ta)
    }
}
