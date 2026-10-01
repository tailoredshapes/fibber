//! The context lines of the expansion dump (`--context`,
//! spec/bootstrap.md §5): what an `ExpandCtx` remembers after a module is
//! expanded, in an order that does not depend on the maps it is kept in
//! (names sorted as byte strings), so that a self-hosted expander's
//! context can be compared with this one's line by line.
//!
//! The prelude's own types are most of any context (214 lines of the
//! prelude's), so a program's dump lists only the macros, structs and enums that are not
//! as they were when the prelude had been expanded (a [`Baseline`]): new
//! ones and redefined ones. The dump of a prelude lists them all.

use std::collections::HashMap;
use std::fmt::Write;

use crate::dump::{dump_form, quote, span_in};
use crate::expand::{EnumInfo, ExpandCtx, MacroDef, StructInfo};
use crate::syntax::Form;

/// What the context held when the entries it lists are compared with:
/// the text of each macro, struct and enum by what it is and its name,
/// as [`entry_text`] writes it with no home file (so every position
/// names its file and the text does not depend on which module is being
/// dumped).
#[derive(Default)]
pub(super) struct Baseline(HashMap<(&'static str, String), String>);

impl Baseline {
    /// No entry: everything the context holds is listed.
    pub(super) fn empty() -> Baseline {
        Baseline::default()
    }

    /// The entries of `ctx` as they are.
    pub(super) fn of(ctx: &ExpandCtx) -> Baseline {
        Baseline(
            entries(ctx)
                .into_iter()
                .map(|e| ((e.kind(), e.name()), e.text("")))
                .collect(),
        )
    }

    fn holds(&self, e: &Entry) -> bool {
        self.0.get(&(e.kind(), e.name())).map(String::as_str) == Some(e.text("").as_str())
    }
}

/// One macro, struct or enum of the context.
enum Entry<'a> {
    Macro(&'a MacroDef),
    Struct(&'a StructInfo),
    Enum(&'a EnumInfo),
}

impl Entry<'_> {
    fn kind(&self) -> &'static str {
        match self {
            Entry::Macro(_) => "macro",
            Entry::Struct(_) => "struct",
            Entry::Enum(_) => "enum",
        }
    }

    /// The name it is kept under: a macro's `ns/name`.
    fn name(&self) -> String {
        match self {
            Entry::Macro(m) => m.key.clone(),
            Entry::Struct(s) => s.name.clone(),
            Entry::Enum(e) => e.name.clone(),
        }
    }

    /// Its lines, with positions in files other than `home` ending
    /// `@FILE`.
    fn text(&self, home: &str) -> String {
        let mut out = String::new();
        match self {
            Entry::Macro(m) => macro_lines(m, home, &mut out),
            Entry::Struct(s) => struct_lines(s, home, &mut out),
            Entry::Enum(e) => enum_lines(e, home, &mut out),
        }
        out
    }
}

/// The macros by key, then the structs by name, then the enums by name.
fn entries(ctx: &ExpandCtx) -> Vec<Entry<'_>> {
    let mut all: Vec<Entry> = ctx.macros_by_key().into_iter().map(Entry::Macro).collect();
    all.extend(ctx.structs_by_name().into_iter().map(Entry::Struct));
    all.extend(ctx.enums_by_name().into_iter().map(Entry::Enum));
    all
}

/// Appends the context of `ctx`, the module `ns` of file `home` just
/// expanded (and not yet ended), to `out`: what is not in `baseline`.
/// Positions in other files than `home` end with `@FILE`, as in the dump
/// of the forms.
pub(super) fn dump_context(
    ctx: &ExpandCtx,
    ns: &str,
    home: &str,
    baseline: &Baseline,
    out: &mut String,
) {
    let _ = writeln!(out, "-- context {ns}");
    let _ = writeln!(out, "gensyms {}", ctx.gensym_count());
    let (steps, forms) = ctx.counters();
    let _ = writeln!(out, "counters steps {steps} forms {forms}");
    scope_lines(ctx, out);
    for name in ctx.private_type_names() {
        let _ = writeln!(out, "private {}", quote(name));
    }
    for name in ctx.hidden_type_names() {
        let _ = writeln!(out, "hidden {}", quote(name));
    }
    for e in entries(ctx).iter().filter(|e| !baseline.holds(e)) {
        out.push_str(&e.text(home));
    }
}

/// The module being expanded as macro lookup sees it: its name, then the
/// modules it uses in the order written, then its aliases by alias; then
/// the modules each module re-exports, by module.
fn scope_lines(ctx: &ExpandCtx, out: &mut String) {
    let scope = ctx.scope();
    let _ = writeln!(out, "scope {}", quote(&scope.ns));
    for u in &scope.uses {
        let _ = writeln!(out, "  use {}", quote(u));
    }
    let mut aliases: Vec<(&String, &String)> = scope.aliases.iter().collect();
    aliases.sort();
    for (alias, ns) in aliases {
        let _ = writeln!(out, "  alias {} {}", quote(alias), quote(ns));
    }
    for (ns, list) in ctx.reexports() {
        let _ = write!(out, "reexport {}", quote(ns));
        for n in list {
            let _ = write!(out, " {}", quote(n));
        }
        out.push('\n');
    }
}

fn forms_at(forms: &[Form], depth: usize, home: &str, out: &mut String) {
    for f in forms {
        dump_form(f, depth, Some(home), out);
    }
}

/// One macro: its key, whether it is private, its position, its
/// parameters (and rest parameter) and its expanded body.
fn macro_lines(m: &MacroDef, home: &str, out: &mut String) {
    let private = if m.private { "private" } else { "public" };
    let _ = writeln!(out, "macro {} {private}", quote(&m.key));
    let _ = writeln!(out, "  at {}", span_in(&m.pos, home));
    out.push_str("  params");
    for p in &m.params {
        let _ = write!(out, " {}", quote(p));
    }
    out.push('\n');
    match &m.rest {
        Some(r) => {
            let _ = writeln!(out, "  rest {}", quote(r));
        }
        None => out.push_str("  rest -\n"),
    }
    let _ = writeln!(out, "  body {}", m.body.len());
    forms_at(&m.body, 2, home, out);
}

/// One struct: its parameters, then each field as the name form and the
/// type form.
fn struct_lines(s: &StructInfo, home: &str, out: &mut String) {
    let _ = writeln!(out, "struct {}", quote(&s.name));
    let _ = writeln!(out, "  params {}", s.params.len());
    forms_at(&s.params, 2, home, out);
    let _ = writeln!(out, "  fields {}", s.fields.len());
    for (name, ty) in &s.fields {
        dump_form(name, 2, Some(home), out);
        dump_form(ty, 2, Some(home), out);
    }
}

/// One enum: its parameters, then each variant with its fields (the name
/// when written, `-` when not, and the type form below it).
fn enum_lines(e: &EnumInfo, home: &str, out: &mut String) {
    let _ = writeln!(out, "enum {}", quote(&e.name));
    let _ = writeln!(out, "  params {}", e.params.len());
    forms_at(&e.params, 2, home, out);
    let _ = writeln!(out, "  variants {}", e.variants.len());
    for v in &e.variants {
        let _ = writeln!(out, "    variant {} {}", quote(&v.name), v.fields.len());
        for (name, ty) in &v.fields {
            match name {
                Some(n) => {
                    let _ = writeln!(out, "      field {}", quote(n));
                }
                None => out.push_str("      field -\n"),
            }
            dump_form(ty, 4, Some(home), out);
        }
    }
}
