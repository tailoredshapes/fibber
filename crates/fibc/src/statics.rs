//! Static data (types §8.2): string literals, keyword ids, the
//! immortal closures of named functions, and vtables, each an lIR
//! `constant` with count 0 and the `IMMORTAL` flag.

use std::collections::HashMap;
use std::fmt::Write;

use crate::layout::IMMORTAL;

/// The static objects of a program.
#[derive(Debug, Default)]
pub struct Statics {
    strings: Vec<(String, Vec<u8>)>,
    string_index: HashMap<Vec<u8>, String>,
    keywords: Vec<String>,
    keyword_index: HashMap<String, i64>,
    closures: Vec<(String, String, u32)>,
    closure_index: HashMap<String, String>,
    vtables: Vec<(String, Vec<String>)>,
    vtable_index: HashMap<String, String>,
}

impl Statics {
    /// The address of the immortal `str` object holding `text`.
    pub fn string(&mut self, text: &str, tid_str: u32) -> String {
        let bytes = text.as_bytes().to_vec();
        if let Some(n) = self.string_index.get(&bytes) {
            return n.clone();
        }
        let name = format!("@str.{}", self.strings.len());
        self.strings.push((format!("{tid_str}"), bytes.clone()));
        self.string_index.insert(bytes, name.clone());
        name
    }

    /// The interned id of a keyword.
    pub fn keyword(&mut self, k: &str) -> i64 {
        if let Some(id) = self.keyword_index.get(k) {
            return *id;
        }
        let id = self.keywords.len() as i64;
        self.keywords.push(k.to_string());
        self.keyword_index.insert(k.to_string(), id);
        id
    }

    /// Every keyword interned so far, by id.
    pub fn keywords(&self) -> Vec<String> {
        self.keywords.clone()
    }

    /// The address of the immortal closure whose code is `code` (§8.4).
    pub fn closure(&mut self, code: &str, tid: u32) -> String {
        if let Some(n) = self.closure_index.get(code) {
            return n.clone();
        }
        let name = format!("@clo.{}", self.closures.len());
        self.closures.push((name.clone(), code.to_string(), tid));
        self.closure_index.insert(code.to_string(), name.clone());
        name
    }

    /// The address of the vtable named `name` with these slots (§8.5),
    /// or of the one registered under that name before.
    pub fn vtable(&mut self, name: &str, slots: Vec<String>) -> String {
        if let Some(n) = self.vtable_index.get(name) {
            return n.clone();
        }
        let global = format!("@vt.{}", self.vtables.len());
        self.vtables.push((global.clone(), slots));
        self.vtable_index.insert(name.to_string(), global.clone());
        global
    }

    /// Whether a vtable of this name exists already.
    pub fn has_vtable(&self, name: &str) -> bool {
        self.vtable_index.contains_key(name)
    }

    /// The constants.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, (tid, bytes)) in self.strings.iter().enumerate() {
            let n = bytes.len();
            let elems: Vec<String> = bytes
                .iter()
                .chain(std::iter::once(&0u8))
                .map(|b| format!("(i8 {b})"))
                .collect();
            let _ = writeln!(
                out,
                "(constant internal str.{i} {{ i64 i32 i32 i64 [{} x i8] }} {{ (i64 0) (i32 {tid}) (i32 {IMMORTAL}) (i64 {n}) ([{} x i8] {}) }})",
                n + 1,
                n + 1,
                elems.join(" ")
            );
        }
        for (name, code, tid) in &self.closures {
            let _ = writeln!(
                out,
                "(constant internal {} {{ i64 i32 i32 ptr }} {{ (i64 0) (i32 {tid}) (i32 {IMMORTAL}) @{code} }})",
                &name[1..]
            );
        }
        for (name, slots) in &self.vtables {
            let _ = writeln!(
                out,
                "(constant internal {} [{} x ptr] ([{} x ptr] {}))",
                &name[1..],
                slots.len(),
                slots.len(),
                slots.join(" ")
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_shared_and_render_as_constants() {
        let mut s = Statics::default();
        assert_eq!(s.string("hi", 0), "@str.0");
        assert_eq!(s.string("hi", 0), "@str.0");
        assert_eq!(s.string("", 0), "@str.1");
        assert_eq!(s.keyword("a"), 0);
        assert_eq!(s.keyword("b"), 1);
        assert_eq!(s.keyword("a"), 0);
        assert_eq!(s.closure("f.g.owned", 3), "@clo.0");
        s.vtable("Show.str", vec!["@m.Show.show.str".into()]);
        let text = s.render();
        assert!(text.contains(
            "(constant internal str.0 { i64 i32 i32 i64 [3 x i8] } { (i64 0) (i32 0) (i32 8) (i64 2) ([3 x i8] (i8 104) (i8 105) (i8 0)) })"
        ));
        let src = format!(
            "(define tailcc (f.g.owned i64) ((ptr e)) (block entry (ret (i64 1))))\n(define tailcc (m.Show.show.str ptr) ((ptr s)) (block entry (ret s)))\n{text}"
        );
        if let Err(e) = lir::parse_and_check(&src) {
            panic!("{}\n{src}", e[0]);
        }
    }
}
