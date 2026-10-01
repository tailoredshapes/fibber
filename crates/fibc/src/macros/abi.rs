//! The C-callable surface of a macro-time module (compiler.md §6):
//! the entry that runs the macro, constructors and readers of `Form`
//! objects for the expander's forms, the table of the keywords the
//! module interned (an `Int` or `Flt` form's width is a keyword id), and
//! the hooks the module calls back for `gensym` and reflection. All
//! `ccc` and exported.

use std::fmt::Write;

use crate::layout::IMMORTAL;
use crate::lower::pattern::VecIds;

/// What the module's `Form` layout is, for the ABI text.
pub struct FormLayout {
    /// The `Form` object's struct name (its variants are `.vN`).
    pub sname: String,
    pub tid: u32,
    /// The variant index of each name: Sym Kw Int Flt Str Chr Bool Nil
    /// List Vec Map.
    pub tags: [u32; 11],
    /// The size of a `Form` object.
    pub size: u64,
    pub vec: VecIds,
    /// The static `str` constant (`@str.N`) holding each interned
    /// keyword's name, by keyword id.
    pub keyword_strs: Vec<String>,
}

/// The width suffixes `compile_macro` interns before anything else, so
/// that a form of any width can be given an id by name.
pub const WIDTHS: [&str; 6] = ["i8", "i16", "i32", "i64", "f32", "f64"];

/// The variant names in the order of [`FormLayout::tags`].
pub const VARIANTS: [&str; 11] = [
    "Sym", "Kw", "Int", "Flt", "Str", "Chr", "Bool", "Nil", "List", "Vec", "Map",
];

/// The lIR of the module's surface; `entry` runs the macro body
/// `body` on `n` `Form` (or rest vector) arguments.
pub fn render(f: &FormLayout, body: &str, n: usize, k: usize) -> String {
    let mut s = render_keywords(&f.keyword_strs, k);
    let params: Vec<String> = (0..n).map(|i| format!("(ptr a{i})")).collect();
    let args: Vec<String> = (0..n).map(|i| format!("a{i}")).collect();
    let _ = writeln!(
        s,
        "(define (fibm.entry.{k} ptr) ({})\n  (block entry (ret (call @{body} {}))))",
        params.join(" "),
        args.join(" ")
    );
    let sn = &f.sname;
    let _ = writeln!(
        s,
        "(define (fibm.str.{k} ptr) ((ptr bytes) (i64 n))
  (block entry
    (let ((r (call @fib.str-new n)))
      (call @memcpy (call @fib.str-bytes-ptr r) bytes n)
      (ret r))))
(define (fibm.form.{k} ptr) ((i32 tag) (i64 a) (i64 b) (ptr p))
  (block entry
    (let ((o (call @fib.alloc (i64 {}) (i32 {})))
          (f0 (getelementptr %struct.{sn}.v0 o (i32 0) (i32 4)))
          (f1 (getelementptr %struct.{sn}.v2 o (i32 0) (i32 5))))
      (store (i32 {IMMORTAL}) (getelementptr %struct.fib.hdr o (i32 0) (i32 2)))
      (store (i64 0) (getelementptr %struct.fib.hdr o (i32 0) (i32 0)))
      (store tag (getelementptr %struct.{sn} o (i32 0) (i32 3)))
      (switch tag done ((i32 {}) ptrf) ((i32 {}) ptrf) ((i32 {}) intf) ((i32 {}) fltf) ((i32 {}) ptrf) ((i32 {}) chrf) ((i32 {}) boolf) ((i32 {}) ptrf) ((i32 {}) ptrf) ((i32 {}) ptrf))))
  (block ptrf (store p f0) (br done))
  (block intf (store a f0) (store b f1) (br done))
  (block fltf (store (bitcast double a) f0) (store b f1) (br done))
  (block chrf (store (trunc i32 a) (getelementptr %struct.{sn}.v5 o (i32 0) (i32 4))) (br done))
  (block boolf (store (trunc i1 a) (getelementptr %struct.{sn}.v6 o (i32 0) (i32 4))) (br done))
  (block done (ret o)))
(define (fibm.vec.{k} ptr) ((ptr buf) (i64 n))
  (block entry (ret (call @fib.vec-build buf n (i64 8) (i8 1) (i32 {}) (i32 {}) (i32 {}) (i32 {})))))
(define (fibm.tag.{k} i32) ((ptr o))
  (block entry (ret (load i32 (getelementptr %struct.{sn} o (i32 0) (i32 3))))))
(define (fibm.f0-ptr.{k} ptr) ((ptr o))
  (block entry (ret (load ptr (getelementptr %struct.{sn}.v0 o (i32 0) (i32 4))))))
(define (fibm.f0-i64.{k} i64) ((ptr o))
  (block entry (ret (load i64 (getelementptr %struct.{sn}.v2 o (i32 0) (i32 4))))))
(define (fibm.f0-f64.{k} double) ((ptr o))
  (block entry (ret (load double (getelementptr %struct.{sn}.v3 o (i32 0) (i32 4))))))
(define (fibm.f0-i32.{k} i32) ((ptr o))
  (block entry (ret (load i32 (getelementptr %struct.{sn}.v5 o (i32 0) (i32 4))))))
(define (fibm.f0-i1.{k} i32) ((ptr o))
  (block entry (ret (zext i32 (load i1 (getelementptr %struct.{sn}.v6 o (i32 0) (i32 4)))))))
(define (fibm.f1-i64.{k} i64) ((ptr o))
  (block entry (ret (load i64 (getelementptr %struct.{sn}.v2 o (i32 0) (i32 5))))))
(define (fibm.str-len.{k} i64) ((ptr s)) (block entry (ret (call @fib.str-len s))))
(define (fibm.str-ptr.{k} ptr) ((ptr s)) (block entry (ret (call @fib.str-bytes-ptr s))))
(define (fibm.vec-len.{k} i64) ((ptr v)) (block entry (ret (call @fib.vec-len v))))
(define (fibm.vec-elem.{k} ptr) ((ptr v) (i64 i))
  (block entry (ret (load ptr (call @fib.vec-elem-ptr v i (i64 8))))))
(define (fibm.set-hooks.{k} void) ((ptr gensym) (ptr reflect) (ptr cx))
  (block entry (store gensym @fibm.gensym-hook) (store reflect @fibm.reflect-hook) (store cx @fibm.hook-cx) (ret)))
(define (fibm.init.{k} void) () (block entry (call @fib.init) (ret)))",
        f.size,
        f.tid,
        f.tags[0],
        f.tags[1],
        f.tags[2],
        f.tags[3],
        f.tags[4],
        f.tags[5],
        f.tags[6],
        f.tags[8],
        f.tags[9],
        f.tags[10],
        f.vec.tvec,
        f.vec.tnode,
        f.vec.tarr,
        f.vec.tnarr
    );
    s
}

/// `fibm.kw-count.K`, the number of keywords the module interned, and
/// `fibm.kw.K`, the name of the keyword with a given id as an immortal
/// `str` object, or null for an id that is none (below 0 or from the
/// count up). The ids are 0 to count - 1, in the order interned.
fn render_keywords(strs: &[String], k: usize) -> String {
    let cases: String = (0..strs.len())
        .map(|i| format!(" ((i64 {i}) kw{i})"))
        .collect();
    let blocks: String = strs
        .iter()
        .enumerate()
        .map(|(i, s)| format!("\n  (block kw{i} (ret {s}))"))
        .collect();
    format!(
        "(define (fibm.kw-count.{k} i64) () (block entry (ret (i64 {}))))
(define (fibm.kw.{k} ptr) ((i64 id))
  (block entry (switch id none{cases})){blocks}
  (block none (ret (ptr null))))
",
        strs.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statics::Statics;

    #[test]
    fn the_keyword_table_is_a_count_and_a_switch_over_static_names() {
        let mut statics = Statics::default();
        let strs: Vec<String> = ["i8", "f16"].iter().map(|k| statics.string(k, 0)).collect();
        let text = render_keywords(&strs, 3);
        assert_eq!(
            text,
            "(define (fibm.kw-count.3 i64) () (block entry (ret (i64 2))))
(define (fibm.kw.3 ptr) ((i64 id))
  (block entry (switch id none ((i64 0) kw0) ((i64 1) kw1)))
  (block kw0 (ret @str.0))
  (block kw1 (ret @str.1))
  (block none (ret (ptr null))))
"
        );
        let module = format!("{}{text}", statics.render());
        if let Err(e) = lir::parse_and_check(&module) {
            panic!("{}\n{module}", e[0]);
        }
    }

    #[test]
    fn a_table_of_no_keywords_is_a_zero_count_and_a_null_for_every_id() {
        let text = render_keywords(&[], 0);
        assert!(text.contains("(ret (i64 0))"), "{text}");
        if let Err(e) = lir::parse_and_check(&text) {
            panic!("{}\n{text}", e[0]);
        }
    }
}
