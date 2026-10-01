//! A read-only view of what an [`ExpandCtx`] remembers, in an order that
//! does not depend on the hash maps it is kept in: the expansion dump
//! (spec/bootstrap.md §5) prints it, so that the context a self-hosted
//! expander leaves behind can be compared with this one.

use super::ctx::ExpandCtx;
use super::runner::MacroDef;
use super::types::{EnumInfo, StructInfo};

impl ExpandCtx {
    /// Every user macro, by its `ns/name` key, in the order of those keys
    /// as byte strings.
    pub fn macros_by_key(&self) -> Vec<&MacroDef> {
        let mut all: Vec<&MacroDef> = self.macros.values().collect();
        all.sort_by(|a, b| a.key.cmp(&b.key));
        all
    }

    /// Every struct registered, the built-in ones included, by name.
    pub fn structs_by_name(&self) -> Vec<&StructInfo> {
        let mut all: Vec<&StructInfo> = self.types.structs.values().collect();
        all.sort_by(|a, b| a.name.cmp(&b.name));
        all
    }

    /// Every enum registered (`Option` and `Form` among them), by name.
    pub fn enums_by_name(&self) -> Vec<&EnumInfo> {
        let mut all: Vec<&EnumInfo> = self.types.enums.values().collect();
        all.sort_by(|a, b| a.name.cmp(&b.name));
        all
    }

    /// The names of the `:private` structs and enums of the module being
    /// expanded, sorted.
    pub fn private_type_names(&self) -> Vec<&str> {
        sorted(self.types.private.iter())
    }

    /// The names of the private structs and enums of the modules expanded
    /// before this one, which reflection and `derive` no longer see,
    /// sorted.
    pub fn hidden_type_names(&self) -> Vec<&str> {
        sorted(self.types.hidden.iter())
    }
}

fn sorted<'a>(names: impl Iterator<Item = &'a String>) -> Vec<&'a str> {
    let mut all: Vec<&str> = names.map(String::as_str).collect();
    all.sort_unstable();
    all
}

#[cfg(test)]
mod tests {
    use super::super::{expand_program, NoRunner};
    use super::*;
    use crate::syntax::read_all;

    fn expanded(source: &str) -> ExpandCtx {
        let mut ctx = ExpandCtx::new();
        ctx.begin_module("main", &[], Default::default());
        let forms = read_all(source, "t").expect("reads");
        expand_program(forms, &mut ctx, &mut NoRunner).expect("expands");
        ctx
    }

    #[test]
    fn what_the_context_holds_comes_out_sorted_whatever_the_order_of_definition() {
        let ctx = expanded(
            "(defstruct Zed (a: i64)) (defstruct Alpha (b: i64)) \
             (defenum Mid (X) (Y)) (defmacro zz () 1) (defmacro aa () 2) \
             (defmacro mm :private () 3)",
        );
        let structs: Vec<&str> = ctx
            .structs_by_name()
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(structs, ["Alpha", "Zed"]);
        let enums: Vec<&str> = ctx
            .enums_by_name()
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(enums, ["Form", "Mid", "Option"]);
        let keys: Vec<&str> = ctx.macros_by_key().iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, ["main/aa", "main/mm", "main/zz"]);
    }

    #[test]
    fn private_types_move_from_private_to_hidden_when_the_module_ends() {
        let mut ctx = expanded("(defstruct Hid :private (a: i64)) (defstruct Open (a: i64))");
        assert_eq!(ctx.private_type_names(), ["Hid"]);
        assert!(ctx.hidden_type_names().is_empty());
        ctx.end_module();
        assert!(ctx.private_type_names().is_empty());
        assert_eq!(ctx.hidden_type_names(), ["Hid"]);
    }

    #[test]
    fn the_counters_and_the_gensyms_are_those_of_the_context() {
        let ctx = expanded("(defun f (x: i64) -> i64 (when true 1 2))");
        let (steps, forms) = ctx.counters();
        assert_eq!(steps, 1);
        assert!(forms > 1, "{forms}");
        assert_eq!(ctx.gensym_count(), 0);
        let g = ctx.gensym("m", ctx.call_pos());
        assert_eq!(g.as_sym(), Some("#m.1"));
        assert_eq!(ctx.gensym_count(), 1);
    }
}
