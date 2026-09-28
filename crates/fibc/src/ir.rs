//! A builder for the text of one lIR function (spec/lir.md §5): blocks,
//! fresh SSA names, `let`s that keep nesting shallow, `phi`s first in
//! their block, and `alloca`s in the entry block for stack objects
//! (types §8.2).

use std::fmt::Write;

pub use crate::value::{LirTy, V};

/// One instruction of a block, as it will be printed.
#[derive(Clone, Debug)]
enum Item {
    /// A value bound to a fresh name.
    Bind(String, String),
    /// A void instruction, or the terminator.
    Stmt(String),
}

#[derive(Clone, Debug, Default)]
struct Block {
    label: String,
    phis: Vec<(String, String)>,
    items: Vec<Item>,
    closed: bool,
}

/// The function under construction.
#[derive(Debug)]
pub struct FnBuilder {
    pub name: String,
    pub ret: Option<LirTy>,
    pub params: Vec<(LirTy, String)>,
    pub tailcc: bool,
    blocks: Vec<Block>,
    cur: usize,
    counter: u32,
    allocas: Vec<(String, String)>,
}

impl FnBuilder {
    /// A function with its entry block open.
    pub fn new(name: &str, ret: Option<LirTy>, params: Vec<(LirTy, String)>, tailcc: bool) -> Self {
        FnBuilder {
            name: name.to_string(),
            ret,
            params,
            tailcc,
            blocks: vec![Block {
                label: "entry".into(),
                ..Block::default()
            }],
            cur: 0,
            counter: 0,
            allocas: Vec::new(),
        }
    }

    /// A fresh SSA name.
    pub fn fresh(&mut self) -> String {
        self.counter += 1;
        format!("t{}", self.counter)
    }

    /// A fresh block label with `hint` in it.
    pub fn label(&mut self, hint: &str) -> String {
        self.counter += 1;
        format!("{hint}{}", self.counter)
    }

    /// The label of the block instructions go to now.
    pub fn current(&self) -> String {
        self.blocks[self.cur].label.clone()
    }

    /// Whether the current block already ended in a terminator.
    pub fn closed(&self) -> bool {
        self.blocks[self.cur].closed
    }

    /// Opens a new block and makes it current.
    pub fn open(&mut self, label: &str) {
        self.blocks.push(Block {
            label: label.to_string(),
            ..Block::default()
        });
        self.cur = self.blocks.len() - 1;
    }

    /// Binds `instr`'s value to a fresh name and returns it.
    pub fn val(&mut self, instr: &str, ty: LirTy) -> V {
        let n = self.fresh();
        self.push(Item::Bind(n.clone(), instr.to_string()));
        V::Val(n, ty)
    }

    /// Emits a void instruction.
    pub fn stmt(&mut self, instr: &str) {
        self.push(Item::Stmt(instr.to_string()));
    }

    /// Emits the terminator of the current block.
    pub fn term(&mut self, instr: &str) {
        self.push(Item::Stmt(instr.to_string()));
        self.blocks[self.cur].closed = true;
    }

    fn push(&mut self, item: Item) {
        let b = &mut self.blocks[self.cur];
        if !b.closed {
            b.items.push(item);
        }
    }

    /// A `phi` at the head of the current block.
    pub fn phi(&mut self, ty: LirTy, incoming: &[(String, String)]) -> V {
        let n = self.fresh();
        let mut s = format!("(phi {}", ty.text());
        for (label, v) in incoming {
            let _ = write!(s, " ({label} {v})");
        }
        s.push(')');
        self.blocks[self.cur].phis.push((n.clone(), s));
        V::Val(n, ty)
    }

    /// Adds an incoming edge to the `phi` bound to `name` in block
    /// `label` (a loop's back edge, known only once the body is done).
    pub fn patch_phi(&mut self, label: &str, name: &str, from: &str, value: &str) {
        for b in &mut self.blocks {
            if b.label != label {
                continue;
            }
            for (n, text) in &mut b.phis {
                if n == name {
                    text.pop();
                    let _ = write!(text, " ({from} {value}))");
                }
            }
        }
    }

    /// An `alloca` of `ty` in the entry block, reused by every
    /// execution of its site (types §8.2).
    pub fn entry_alloca(&mut self, ty: &str) -> String {
        let n = self.fresh();
        self.allocas.push((n.clone(), format!("(alloca {ty})")));
        n
    }

    /// The function's text.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let cc = if self.tailcc { "tailcc " } else { "" };
        let ret = self.ret.map_or("void", LirTy::text);
        let params: Vec<String> = self
            .params
            .iter()
            .map(|(t, n)| format!("({} {n})", t.text()))
            .collect();
        let _ = writeln!(
            out,
            "(define internal {cc}({} {ret}) ({})",
            self.name,
            params.join(" ")
        );
        for (i, b) in self.blocks.iter().enumerate() {
            let _ = writeln!(out, "  (block {}", b.label);
            let mut binds: Vec<(String, String)> = b.phis.clone();
            if i == 0 {
                binds.extend(self.allocas.iter().cloned());
            }
            render_items(&mut out, &mut binds, &b.items);
            out.push_str("  )\n");
        }
        // The define's closing paren: the last line becomes `  ))`.
        out.pop();
        out.push_str(")\n");
        out
    }
}

/// Prints a block's items: bindings accumulate into one `let` whose
/// body is the next void instruction, so a `let` never nests in a
/// `let` and the block stays within lIR's nesting limit (§1).
fn render_items(out: &mut String, binds: &mut Vec<(String, String)>, items: &[Item]) {
    for item in items {
        match item {
            Item::Bind(n, v) => binds.push((n.clone(), v.clone())),
            Item::Stmt(s) => {
                if binds.is_empty() {
                    let _ = writeln!(out, "    {s}");
                } else {
                    out.push_str("    (let (");
                    for (i, (n, v)) in binds.iter().enumerate() {
                        let sep = if i == 0 { "" } else { "\n          " };
                        let _ = write!(out, "{sep}({n} {v})");
                    }
                    let _ = writeln!(out, ")\n      {s})");
                    binds.clear();
                }
            }
        }
    }
    // A block with bindings but no statement after them is an
    // unterminated block, which lIR's checker reports; the bindings
    // are printed so that the diagnostic names the function.
    if !binds.is_empty() {
        out.push_str("    (let (");
        for (n, v) in binds.iter() {
            let _ = write!(out, "({n} {v}) ");
        }
        out.push_str(") (unreachable))\n");
        binds.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_lets_around_void_instructions() {
        let mut f = FnBuilder::new("f", Some(LirTy::I64), vec![(LirTy::I64, "x".into())], true);
        let a = f.val("(add x (i64 1))", LirTy::I64);
        f.stmt("(call @g)");
        let b = f.val(&format!("(mul {} (i64 2))", a.text()), LirTy::I64);
        f.term(&format!("(ret {})", b.text()));
        let text = f.render();
        assert!(text.starts_with("(define internal tailcc (f i64) ((i64 x))\n  (block entry\n"));
        assert!(text.contains("(let ((t1 (add x (i64 1))))\n      (call @g))"));
        assert!(text.contains("(let ((t2 (mul t1 (i64 2))))\n      (ret t2))"));
        assert!(text.ends_with("  ))\n"));
        assert!(lir::parse(&text).is_ok(), "{text}");
    }

    #[test]
    fn phis_and_allocas_come_first() {
        let mut f = FnBuilder::new("g", None, vec![], false);
        let slot = f.entry_alloca("i64");
        f.stmt(&format!("(store (i64 0) {slot})"));
        f.term("(br next)");
        f.open("next");
        let p = f.phi(LirTy::I64, &[("entry".into(), "(i64 1)".into())]);
        f.term(&format!("(store {} {slot})", p.text()));
        let text = f.render();
        assert!(text.contains("(let ((t1 (alloca i64)))\n      (store (i64 0) t1))"));
        assert!(text.contains("(block next\n    (let ((t2 (phi i64 (entry (i64 1)))))"));
        assert!(lir::parse(&text).is_ok(), "{text}");
    }
}
