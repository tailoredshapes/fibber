//! A builder for the text of one lIR function (spec/lir.md §5): blocks,
//! fresh SSA names, `let`s that keep nesting shallow, `phi`s first in
//! their block, and `alloca`s in the entry block for stack objects
//! (types §8.2). A block that no live branch names is dead, and what
//! is emitted into it is discarded: the lowering keeps emitting after
//! a call that does not return, and lIR refuses a use of a name bound
//! in code nothing reaches (lir.md §5).

use std::collections::HashSet;
use std::fmt::Write;

pub use crate::value::{LirTy, V};

/// One instruction of a block, as it will be printed.
#[derive(Clone, Debug)]
pub(crate) enum Item {
    /// A value of the type bound to a fresh name.
    Bind(String, String, LirTy),
    /// A void instruction, or the terminator.
    Stmt(String),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Block {
    pub(crate) label: String,
    /// Each `phi`: its name, its text, its type.
    pub(crate) phis: Vec<(String, String, LirTy)>,
    pub(crate) items: Vec<Item>,
    pub(crate) closed: bool,
    /// Opened with no live branch to it: kept out of the text.
    pub(crate) dead: bool,
}

/// The function under construction.
#[derive(Debug)]
pub struct FnBuilder {
    pub name: String,
    pub ret: Option<LirTy>,
    pub params: Vec<(LirTy, String)>,
    pub tailcc: bool,
    pub(crate) blocks: Vec<Block>,
    cur: usize,
    counter: u32,
    /// Each `alloca`: its name, its text, the size of what it holds.
    pub(crate) allocas: Vec<(String, String, u64)>,
    /// The labels a live terminator has named so far.
    live_targets: HashSet<String>,
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
            live_targets: HashSet::new(),
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

    /// Opens a new block and makes it current; dead unless a live
    /// terminator has named it.
    pub fn open(&mut self, label: &str) {
        let dead = !self.live_targets.contains(label);
        self.blocks.push(Block {
            label: label.to_string(),
            dead,
            ..Block::default()
        });
        self.cur = self.blocks.len() - 1;
    }

    fn is_dead(&self, label: &str) -> bool {
        self.blocks.iter().any(|b| b.label == label && b.dead)
    }

    /// Binds `instr`'s value to a fresh name and returns it.
    pub fn val(&mut self, instr: &str, ty: LirTy) -> V {
        let n = self.fresh();
        self.push(Item::Bind(n.clone(), instr.to_string(), ty));
        V::Val(n, ty)
    }

    /// Emits a void instruction.
    pub fn stmt(&mut self, instr: &str) {
        self.push(Item::Stmt(instr.to_string()));
    }

    /// Emits the terminator of the current block; the labels it names
    /// are live when the block is.
    pub fn term(&mut self, instr: &str) {
        let b = &self.blocks[self.cur];
        if !b.closed && !b.dead {
            self.live_targets.extend(targets(instr));
        }
        self.push(Item::Stmt(instr.to_string()));
        self.blocks[self.cur].closed = true;
    }

    fn push(&mut self, item: Item) {
        let b = &mut self.blocks[self.cur];
        if !b.closed && !b.dead {
            b.items.push(item);
        }
    }

    /// A `phi` at the head of the current block, without the edges
    /// from dead blocks.
    pub fn phi(&mut self, ty: LirTy, incoming: &[(String, String)]) -> V {
        let n = self.fresh();
        if self.blocks[self.cur].dead {
            return V::Val(n, ty);
        }
        let mut s = format!("(phi {}", ty.text());
        for (label, v) in incoming {
            if !self.is_dead(label) {
                let _ = write!(s, " ({label} {v})");
            }
        }
        s.push(')');
        self.blocks[self.cur].phis.push((n.clone(), s, ty));
        V::Val(n, ty)
    }

    /// Adds an incoming edge to the `phi` bound to `name` in block
    /// `label` (a loop's back edge, known only once the body is done);
    /// no edge comes from a dead block.
    pub fn patch_phi(&mut self, label: &str, name: &str, from: &str, value: &str) {
        if self.is_dead(from) {
            return;
        }
        for b in &mut self.blocks {
            if b.label != label {
                continue;
            }
            for (n, text, _) in &mut b.phis {
                if n == name {
                    text.pop();
                    let _ = write!(text, " ({from} {value}))");
                }
            }
        }
    }

    /// An `alloca` of `ty`, holding `size` bytes, in the entry block,
    /// reused by every execution of its site (types §8.2).
    pub fn entry_alloca(&mut self, ty: &str, size: u64) -> String {
        let n = self.fresh();
        self.allocas
            .push((n.clone(), format!("(alloca {ty})"), size));
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
            if b.dead {
                continue;
            }
            let _ = writeln!(out, "  (block {}", b.label);
            let mut binds: Vec<(String, String)> = b
                .phis
                .iter()
                .map(|(n, t, _)| (n.clone(), t.clone()))
                .collect();
            if i == 0 {
                binds.extend(self.allocas.iter().map(|(n, t, _)| (n.clone(), t.clone())));
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

/// The labels a terminator names: `(br l)`, `(br c l1 l2)`, and a
/// `switch`'s default and case labels; none for `ret` and
/// `unreachable`.
pub(crate) fn targets(term: &str) -> Vec<String> {
    let Sx::List(items) = parse_sx(&mut term.chars().peekable()) else {
        return Vec::new();
    };
    let atom = |s: &Sx| match s {
        Sx::Atom(a) => Some(a.clone()),
        Sx::List(_) => None,
    };
    match items.first().and_then(atom).as_deref() {
        Some("br") => match &items[1..] {
            [l] => atom(l).into_iter().collect(),
            [_, a, b] => [a, b].into_iter().filter_map(atom).collect(),
            _ => Vec::new(),
        },
        Some("switch") => {
            let mut out: Vec<String> = items.get(2).and_then(atom).into_iter().collect();
            for case in items.iter().skip(3) {
                if let Sx::List(c) = case {
                    out.extend(c.last().and_then(atom));
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// The shape of an instruction's text.
pub(crate) enum Sx {
    Atom(String),
    List(Vec<Sx>),
}

pub(crate) fn parse_sx(cs: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Sx {
    while cs.peek().is_some_and(|c| c.is_whitespace()) {
        cs.next();
    }
    if cs.peek() == Some(&'(') {
        cs.next();
        let mut items = Vec::new();
        loop {
            while cs.peek().is_some_and(|c| c.is_whitespace()) {
                cs.next();
            }
            match cs.peek() {
                None => break,
                Some(')') => {
                    cs.next();
                    break;
                }
                Some(_) => items.push(parse_sx(cs)),
            }
        }
        return Sx::List(items);
    }
    let mut a = String::new();
    while let Some(&c) = cs.peek() {
        if c.is_whitespace() || c == '(' || c == ')' {
            break;
        }
        a.push(c);
        cs.next();
    }
    Sx::Atom(a)
}

/// Prints a block's items: bindings accumulate into one `let` whose
/// body is the next void instruction, so a `let` never nests in a
/// `let` and the block stays within lIR's nesting limit (§1).
fn render_items(out: &mut String, binds: &mut Vec<(String, String)>, items: &[Item]) {
    for item in items {
        match item {
            Item::Bind(n, v, _) => binds.push((n.clone(), v.clone())),
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
    fn terminators_name_their_targets() {
        assert_eq!(targets("(br next)"), ["next"]);
        assert_eq!(targets("(br t1 yes no)"), ["yes", "no"]);
        assert_eq!(targets("(br (i1 1) yes no)"), ["yes", "no"]);
        assert_eq!(
            targets("(switch t2 done ((i32 0) v0) ((i32 1) v1))"),
            ["done", "v0", "v1"]
        );
        assert!(targets("(ret t3)").is_empty());
        assert!(targets("(unreachable)").is_empty());
    }

    #[test]
    fn a_block_no_live_branch_names_is_discarded() {
        // After a call that does not return, the lowering still emits
        // the rest of the expression: a branch, its blocks, a join with
        // a phi. None of it may reach the text.
        let mut f = FnBuilder::new("h", Some(LirTy::I64), vec![], false);
        f.stmt("(call @fib.trap-c (string \"boom\"))");
        f.term("(unreachable)");
        let c = f.val("(icmp eq (i64 1) (i64 1))", LirTy::I1);
        f.term(&format!("(br {} yes no)", c.text()));
        f.open("yes");
        f.term("(br join)");
        f.open("no");
        f.term("(br join)");
        f.open("join");
        let p = f.phi(
            LirTy::I64,
            &[
                ("yes".into(), "(i64 6)".into()),
                ("no".into(), "(i64 8)".into()),
            ],
        );
        f.term(&format!("(ret {})", p.text()));
        let text = f.render();
        assert!(!text.contains("(block yes"), "{text}");
        assert!(!text.contains("phi"), "{text}");
        assert!(text.ends_with("(unreachable)\n  ))\n"), "{text}");
        assert!(
            lir::parse_and_check(&format!("(declare fib.trap-c void (ptr))\n{text}")).is_ok(),
            "{text}"
        );
        // A live join keeps only its live edges.
        let mut g = FnBuilder::new("k", Some(LirTy::I64), vec![(LirTy::I1, "c".into())], false);
        g.term("(br c yes done)");
        g.open("yes");
        g.term("(br done)");
        g.open("dead");
        g.term("(br done)");
        g.open("done");
        let p = g.phi(
            LirTy::I64,
            &[
                ("entry".into(), "(i64 1)".into()),
                ("yes".into(), "(i64 2)".into()),
                ("dead".into(), "(i64 3)".into()),
            ],
        );
        g.term(&format!("(ret {})", p.text()));
        let text = g.render();
        assert!(
            text.contains("(phi i64 (entry (i64 1)) (yes (i64 2)))"),
            "{text}"
        );
        assert!(!text.contains("(block dead"), "{text}");
        assert!(lir::parse_and_check(&text).is_ok(), "{text}");
    }

    #[test]
    fn phis_and_allocas_come_first() {
        let mut f = FnBuilder::new("g", None, vec![], false);
        let slot = f.entry_alloca("i64", 8);
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
