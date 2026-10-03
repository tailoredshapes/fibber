//! `fibref complete` and hover: the candidates at a cursor in an editor
//! buffer, and the type of the name under it. The testable core that the
//! language server calls.

use std::collections::HashSet;

use super::analysis::{analyse_at, Analysis};
use super::catalog::{self, Item};
use super::context::{classify, Want};
use super::scope::{scope_at, Scope};
use super::text::offset_of;
use crate::expand::{CORE_FORMS, PRELUDE_MACROS};
use crate::json::Json;
use crate::roots::Roots;
use crate::types::TypedProgram;

/// The candidates of the right class at `line` (1-based) and `col`
/// (0-based) of `src`, the text of the file `path`. Never fails: a
/// buffer that does not read or check gets the scope the reader can
/// give, then the library's names; prefix filtering is the client's.
pub fn complete(src: &str, path: &str, line: usize, col: usize, roots: &Roots) -> Vec<Item> {
    let off = offset_of(src, line, col);
    let ctx = classify(&src[..off]);
    let analysis = analyse_at(src, path, roots, Some(off));
    let scope = scope_at(src, ctx.word_start, path);
    let typed = analysis.typed.as_ref();
    let items = match ctx.want {
        Want::Keyword => keywords(src, ctx.word_start),
        Want::Type => own_types(&scope).chain_with(typed.map(catalog::types)),
        Want::Alias(a) => typed.map(|t| catalog::exports(t, &a)).unwrap_or_default(),
        Want::Field(x) => fields(&x, &scope, &analysis, path, off),
        Want::Name => names(&scope, &analysis, path, off),
    };
    dedupe(items)
}

trait ChainWith {
    fn chain_with(self, more: Option<Vec<Item>>) -> Vec<Item>;
}

impl ChainWith for Vec<Item> {
    fn chain_with(mut self, more: Option<Vec<Item>>) -> Vec<Item> {
        self.extend(more.unwrap_or_default());
        self
    }
}

fn dedupe(items: Vec<Item>) -> Vec<Item> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|i| seen.insert(i.label.clone()))
        .collect()
}

fn own_types(scope: &Scope) -> Vec<Item> {
    own_defs(scope, &["struct", "enum", "protocol"])
}

fn own_defs(scope: &Scope, kinds: &[&str]) -> Vec<Item> {
    scope
        .defs
        .iter()
        .filter(|d| kinds.contains(&d.kind))
        .map(|d| Item::new(&d.name, d.kind, d.detail.clone(), "this module"))
        .collect()
}

/// The keywords written in `src`, but not the word being typed.
fn keywords(src: &str, word_start: usize) -> Vec<Item> {
    let mut out = Vec::new();
    let mut at = 0;
    for word in src.split(|c: char| c.is_whitespace() || "()[]{}\";,'`~@^".contains(c)) {
        let start = at;
        at += word.len() + 1;
        if word.len() > 1 && word.starts_with(':') && start != word_start {
            out.push(Item::new(word, "keyword", String::new(), "this file"));
        }
    }
    out
}

fn names(scope: &Scope, a: &Analysis, path: &str, off: usize) -> Vec<Item> {
    let mut out = Vec::new();
    for l in scope.locals.iter().rev() {
        let detail = a
            .typed
            .as_ref()
            .filter(|_| a.own)
            .and_then(|t| catalog::local_type(t, &l.name, path, off))
            .map(|(_, ty)| ty)
            .or_else(|| l.ann.clone())
            .unwrap_or_default();
        out.push(Item::new(&l.name, "variable", detail, "local"));
    }
    let own: Vec<_> = scope
        .defs
        .iter()
        .map(|d| Item::new(&d.name, d.kind, d.detail.clone(), "this module"))
        .collect();
    out.extend(own);
    out.extend(a.typed.as_ref().map(catalog::names).unwrap_or_default());
    out.extend(
        CORE_FORMS
            .iter()
            .map(|f| Item::new(f, "macro", "special form".into(), "core")),
    );
    out.extend(
        PRELUDE_MACROS
            .iter()
            .map(|m| Item::new(m, "macro", "macro".into(), "prelude")),
    );
    out
}

fn fields(x: &str, scope: &Scope, a: &Analysis, path: &str, off: usize) -> Vec<Item> {
    let Some(t) = a.typed.as_ref() else {
        return Vec::new();
    };
    let checked = Some(a.own)
        .filter(|own| *own)
        .and_then(|_| catalog::local_type(t, x, path, off))
        .and_then(|(id, _)| id);
    let annotated = || {
        let ann = scope
            .locals
            .iter()
            .rev()
            .find(|l| l.name == x)?
            .ann
            .clone()?;
        t.globals.type_name(t.globals.main, &ann)
    };
    match checked.or_else(annotated) {
        Some(id) => catalog::fields(&t.globals, id),
        None => Vec::new(),
    }
}

/// `{"items":[..]}`, the output of `fibref complete`.
pub fn items_json(items: &[Item]) -> Json {
    Json::obj([(
        "items",
        Json::Arr(items.iter().map(Item::to_json).collect()),
    )])
}

/// The word under byte `off`: its start and end.
fn word_at(src: &str, off: usize) -> (usize, usize) {
    let brk = |c: char| c.is_whitespace() || "()[]{}\";,'`~@^".contains(c);
    let start = src[..off]
        .rfind(brk)
        .map_or(0, |i| i + src[i..].chars().next().map_or(1, char::len_utf8));
    let end = src[off..].find(brk).map_or(src.len(), |i| off + i);
    (start, end)
}

/// The text to show when the cursor rests on a name: a local's type or
/// the checker's scheme of a global, as `name : type`; none when it
/// is no name the program knows.
pub fn hover(src: &str, path: &str, line: usize, col: usize, roots: &Roots) -> Option<String> {
    let off = offset_of(src, line, col);
    let (s, e) = word_at(src, off);
    let word = src.get(s..e).filter(|w| !w.is_empty())?;
    let analysis = analyse_at(src, path, roots, Some(off));
    let t = analysis.typed.as_ref()?;
    if let Some((_, ty)) = catalog::local_type(t, word, path, off).filter(|_| analysis.own) {
        return Some(format!("{word} : {ty}"));
    }
    global_text(t, word)
}

fn global_text(t: &TypedProgram, word: &str) -> Option<String> {
    let g = &t.globals;
    let item = match word.split_once('/') {
        Some((alias, name)) => {
            let m = g.module(g.main).aliases.get(alias).copied()?;
            catalog::value_item(t, m, name)?
        }
        None => catalog::value_item(t, g.main, word)?,
    };
    Some(format!(
        "{} : {}\n({}, {})",
        item.label, item.detail, item.kind, item.doc
    ))
}

#[cfg(test)]
mod tests;
