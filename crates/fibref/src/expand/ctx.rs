//! The expansion context: everything the expander remembers between
//! forms, passed explicitly (no global or thread-local state).

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::syntax::{Form, FormKind, Pos};

use super::error::{ExpandError, ExpandErrorKind};
use super::runner::MacroDef;
use super::types::{EnumInfo, StructInfo, TypeTable};

/// The limits that make expansion terminate on every input
/// (implementation limits, syntax §3.16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Most macro expansions (user macros, prelude macros and
    /// quasiquote rewrites) allowed while expanding one top-level form,
    /// a spliced `do` counting as one form. A macro whose expansion
    /// contains its own call again (directly, or through others) hits
    /// this and fails with [`ExpandErrorKind::TooManySteps`].
    pub max_steps: usize,
    /// Deepest nesting of forms the expander will walk or produce. The
    /// walk itself uses an explicit stack, so this does not protect the
    /// expander; it bounds what a macro can build (a macro whose
    /// expansion nests its own call ever deeper fails with
    /// [`ExpandErrorKind::TooDeep`] before it can exhaust the steps) and
    /// the depth later recursive passes over the output (printing,
    /// dropping, checking) must handle. The default is twice the
    /// reader's [`MAX_DEPTH`](crate::syntax::MAX_DEPTH), so every program
    /// the reader accepts is within it.
    pub max_depth: usize,
    /// Most forms (every node of the tree: atoms, lists, vectors and
    /// maps) that the expansions of one top-level form may produce in
    /// all, each expansion's result counted in full. The step limit
    /// alone does not make expansion terminate in practice: a macro
    /// whose result grows at each step (`(defmacro g (... xs) `(g 1
    /// ,@xs))`, or one that doubles its argument) does quadratic or
    /// exponential work long before its steps run out. It fails with
    /// [`ExpandErrorKind::TooLarge`] instead.
    pub max_forms: usize,
}

/// Default for [`Limits::max_steps`].
pub const MAX_STEPS: usize = 100_000;
/// Default for [`Limits::max_depth`].
pub const MAX_EXPAND_DEPTH: usize = 2000;
/// Default for [`Limits::max_forms`]: four million forms, about twice
/// what the longest `and` the depth limit admits produces (about `n²/2`
/// forms for `n` operands).
pub const MAX_EXPANDED_FORMS: usize = 4_000_000;

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_steps: MAX_STEPS,
            max_depth: MAX_EXPAND_DEPTH,
            max_forms: MAX_EXPANDED_FORMS,
        }
    }
}

/// What the expander knows about one module being expanded: the macros
/// defined so far, the structs and enums seen so far, the gensym
/// counter and the limits. A [`MacroRunner`](super::MacroRunner) sees it
/// read-only, and uses it for [`gensym`](Self::gensym), reflection
/// ([`reflect`](Self::reflect)) and the macros it may call.
#[derive(Debug)]
pub struct ExpandCtx {
    /// The limits in force.
    pub limits: Limits,
    /// Gensyms handed out so far. A `Cell` because a runner holds the
    /// context by shared reference while its macro calls `gensym`.
    gensyms: Cell<u64>,
    pub(crate) macros: HashMap<String, MacroDef>,
    pub(crate) types: TypeTable,
    pub(crate) call_pos: Pos,
    pub(crate) steps: usize,
    /// Forms produced by expansions so far, against
    /// [`Limits::max_forms`]; reset with `steps`.
    pub(crate) forms: usize,
}

impl Default for ExpandCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl ExpandCtx {
    /// A context with no user macros, the built-in `Option` and `Form`
    /// enums, and the default [`Limits`].
    pub fn new() -> Self {
        ExpandCtx {
            limits: Limits::default(),
            gensyms: Cell::new(0),
            macros: HashMap::new(),
            types: TypeTable::with_builtins(),
            call_pos: Pos {
                file: Arc::from("<none>"),
                line: 0,
                col: 0,
                start: 0,
                end: 0,
            },
            steps: 0,
            forms: 0,
        }
    }

    /// A fresh symbol (§3.16 `gensym`), at `pos`.
    ///
    /// Its name is `#` + `prefix` + `.` + a counter unique within this
    /// context, e.g. `#m.3`. The reader never produces a symbol starting
    /// with `#` (§1.1: a symbol does not start with `#`; `#` begins a
    /// dispatch), so a gensym cannot collide with a source symbol. Two
    /// gensyms never collide with each other: the text after the last
    /// `.` is the counter, which is different for each.
    pub fn gensym(&self, prefix: &str, pos: &Pos) -> Form {
        let n = self.gensyms.get() + 1;
        self.gensyms.set(n);
        Form::new(FormKind::Sym(format!("#{prefix}.{n}")), pos.clone())
    }

    /// The position of the macro call being expanded: the position a
    /// runner gives the forms it builds that are not input forms (§1.3).
    pub fn call_pos(&self) -> &Pos {
        &self.call_pos
    }

    /// A user macro defined so far, by name.
    pub fn macro_def(&self, name: &str) -> Option<&MacroDef> {
        self.macros.get(name)
    }

    /// A struct seen so far, by name; not a private one of an earlier
    /// module (syntax §5).
    pub fn struct_info(&self, name: &str) -> Option<&StructInfo> {
        self.types
            .structs
            .get(name)
            .filter(|_| !self.types.hidden.contains(name))
    }

    /// An enum seen so far (or `Option`, `Form`), by name; not a
    /// private one of an earlier module (syntax §5).
    pub fn enum_info(&self, name: &str) -> Option<&EnumInfo> {
        self.types
            .enums
            .get(name)
            .filter(|_| !self.types.hidden.contains(name))
    }

    /// Ends the module being expanded: its private structs and enums
    /// are not seen by reflection or `derive` in the modules expanded
    /// after it (syntax §5).
    pub fn end_module(&mut self) {
        self.types.end_module();
    }

    /// Counts one macro expansion against [`Limits::max_steps`].
    pub(crate) fn step(&mut self, pos: &Pos) -> Result<(), ExpandError> {
        self.steps += 1;
        if self.steps > self.limits.max_steps {
            let limit = self.limits.max_steps;
            return Err(ExpandError::new(
                ExpandErrorKind::TooManySteps { limit },
                pos,
            ));
        }
        Ok(())
    }
}
