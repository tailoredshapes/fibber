//! The expansion context: everything the expander remembers between
//! forms, passed explicitly (no global or thread-local state).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::syntax::{Form, FormKind, Pos};

use super::error::{ExpandError, ExpandErrorKind};
use super::fuse::is_library;
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
    /// Gensyms handed out so far. Atomic because a runner holds the
    /// context by shared reference while its macro calls `gensym`, and
    /// the macro's threads share it (one at a time: the evaluator's
    /// turn orders them, so the count is deterministic).
    gensyms: AtomicU64,
    /// Every user macro, by `ns/name`.
    pub(crate) macros: HashMap<String, MacroDef>,
    /// The modules each module re-exports (`(:export-from ..)`), by `ns`.
    exports: HashMap<String, Vec<String>>,
    /// The public names each module defined at top level, by `ns`, as the
    /// modules expanded so far left them (the fusion rewrite asks whether a
    /// module it `:use`s exports a name).
    defined: HashMap<String, HashSet<String>>,
    /// The module being expanded: which macros a name reaches.
    pub(crate) scope: ModuleScope,
    /// The names that hide a prelude macro in the module being expanded:
    /// what it defines itself (`own`) and what the program's own modules
    /// that it `:use`s export. A prelude macro of one of these names
    /// declines in the module.
    hides: HashSet<String>,
    pub(crate) types: TypeTable,
    pub(crate) call_pos: Pos,
    pub(crate) steps: usize,
    /// Forms produced by expansions so far, against
    /// [`Limits::max_forms`]; reset with `steps`.
    pub(crate) forms: usize,
}

/// The module being expanded, for macro lookup (syntax §5): its `ns`,
/// the modules it `:use`s, the implicit modules it sees (after its
/// `:use`s, before the prelude) and its `:require` aliases.
#[derive(Clone, Debug)]
pub struct ModuleScope {
    pub ns: String,
    pub uses: Vec<String>,
    pub implicit: Vec<String>,
    pub aliases: HashMap<String, String>,
}

impl ModuleScope {
    fn of(ns: &str) -> ModuleScope {
        ModuleScope {
            ns: ns.to_string(),
            uses: Vec::new(),
            implicit: Vec::new(),
            aliases: HashMap::new(),
        }
    }
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
            gensyms: AtomicU64::new(0),
            macros: HashMap::new(),
            exports: HashMap::new(),
            defined: HashMap::new(),
            scope: ModuleScope::of(super::PRELUDE_NS),
            hides: HashSet::new(),
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
        let n = self.gensyms.fetch_add(1, Ordering::Relaxed) + 1;
        Form::new(FormKind::Sym(format!("#{prefix}.{n}")), pos.clone())
    }

    /// The position of the macro call being expanded: the position a
    /// runner gives the forms it builds that are not input forms (§1.3).
    pub fn call_pos(&self) -> &Pos {
        &self.call_pos
    }

    /// How many gensyms this context has handed out: the number in the
    /// name of the last one, and the number of the next less one.
    pub fn gensym_count(&self) -> u64 {
        self.gensyms.load(Ordering::Relaxed)
    }

    /// The macro expansions and the forms they produced that the current
    /// (or, after it, the last) top-level form has counted against
    /// [`Limits::max_steps`] and [`Limits::max_forms`]; both start again
    /// at zero with each top-level form.
    pub fn counters(&self) -> (usize, usize) {
        (self.steps, self.forms)
    }

    /// The module being expanded, as macro lookup sees it.
    pub fn scope(&self) -> &ModuleScope {
        &self.scope
    }

    /// The macro `name` that module `ns` exports to the module being
    /// expanded: its own (a `:private` one only to itself), else one that
    /// a module it re-exports exports.
    fn exported_macro(&self, ns: &str, name: &str) -> Option<&MacroDef> {
        let own = self.macros.get(&format!("{ns}/{name}"));
        let own = own.filter(|d| !d.private || d.ns == self.scope.ns);
        own.or_else(|| {
            let reexported = self.exports.get(ns)?;
            reexported.iter().find_map(|r| self.exported_macro(r, name))
        })
    }

    /// The user macro `name` reaches from the module being expanded
    /// (syntax §5): `alias/x` in the module the alias names, or by the
    /// full `ns` of the prelude, of an implicit module or of this module;
    /// a bare name in this module, then in what its `:use`s export, then
    /// in the implicit modules, then in the prelude; a `:private` macro
    /// only in its own module.
    pub fn macro_def(&self, name: &str) -> Option<&MacroDef> {
        let scope = &self.scope;
        if let Some((q, base)) = name.split_once('/') {
            let ns = match scope.aliases.get(q) {
                Some(ns) => ns.as_str(),
                None if q == super::PRELUDE_NS || q == scope.ns => q,
                None if scope.implicit.iter().any(|i| i == q) => q,
                None => return None,
            };
            return self.exported_macro(ns, base);
        }
        std::iter::once(scope.ns.as_str())
            .chain(scope.uses.iter().map(String::as_str))
            .chain(scope.implicit.iter().map(String::as_str))
            .chain(std::iter::once(super::PRELUDE_NS))
            .find_map(|ns| self.exported_macro(ns, name))
    }

    /// The two `:use`d modules of the module being expanded that export
    /// different macros called `name`, which makes the bare name an error
    /// (syntax §5, as for any other name); `None` for a qualified name,
    /// for a name this module defines itself (a local definition shadows
    /// the `:use`d ones) and for a name at most one `:use` exports.
    pub fn macro_ambiguity(&self, name: &str) -> Option<(&str, &str)> {
        let scope = &self.scope;
        let own = self.macros.contains_key(&format!("{}/{name}", scope.ns));
        if name.contains('/') || own {
            return None;
        }
        let mut found = scope
            .uses
            .iter()
            .filter_map(|u| Some((u.as_str(), self.exported_macro(u, name)?)));
        let (first, def) = found.next()?;
        found
            .find(|(_, d)| d.key != def.key)
            .map(|(second, _)| (first, second))
    }

    /// Starts expanding module `ns` with these `:use`s, implicit modules
    /// (`modules::IMPLICIT_LIB`) and aliases.
    pub fn begin_module(
        &mut self,
        ns: &str,
        (uses, implicit): (&[String], &[String]),
        aliases: HashMap<String, String>,
    ) {
        self.scope = ModuleScope {
            ns: ns.to_string(),
            uses: uses.to_vec(),
            implicit: implicit.to_vec(),
            aliases,
        };
    }

    /// Records that module `ns`, which is being expanded or was, re-exports
    /// the modules `exports` (`(:export-from ..)`, syntax §5): the macros
    /// they export are its own to whoever uses it.
    pub fn reexport(&mut self, ns: &str, exports: &[String]) {
        if !exports.is_empty() {
            self.exports.insert(ns.to_string(), exports.to_vec());
        }
    }

    /// Records the public names module `ns` defines at top level.
    pub(crate) fn record_names(&mut self, ns: &str, names: HashSet<String>) {
        self.defined.insert(ns.to_string(), names);
    }

    /// The names module `ns` exports: what it defines publicly and what the
    /// modules it re-exports export, as far as they have been expanded.
    pub(crate) fn exported_names(&self, ns: &str) -> HashSet<String> {
        let mut out = HashSet::new();
        let mut seen = HashSet::new();
        let mut pending = vec![ns];
        while let Some(m) = pending.pop() {
            if !seen.insert(m) {
                continue;
            }
            out.extend(self.defined.get(m).into_iter().flatten().cloned());
            pending.extend(
                self.exports
                    .get(m)
                    .into_iter()
                    .flatten()
                    .map(String::as_str),
            );
        }
        out
    }

    /// The modules each module re-exports, by the module's `ns` (those
    /// that re-export none are not listed), sorted by `ns` as byte
    /// strings; each module's list in the order written.
    pub fn reexports(&self) -> Vec<(&str, &[String])> {
        let mut all: Vec<(&str, &[String])> = self
            .exports
            .iter()
            .map(|(ns, list)| (ns.as_str(), list.as_slice()))
            .collect();
        all.sort_unstable_by_key(|(ns, _)| *ns);
        all
    }

    /// Defines macro `def` in the module being expanded.
    pub(crate) fn define_macro(&mut self, mut def: MacroDef) {
        def.ns = self.scope.ns.clone();
        def.key = format!("{}/{}", self.scope.ns, def.name);
        self.macros.insert(def.key.clone(), def);
    }

    /// Makes the macro `name` of the module being expanded `:private`.
    pub(crate) fn make_macro_private(&mut self, name: &str) {
        if let Some(def) = self.macros.get_mut(&format!("{}/{name}", self.scope.ns)) {
            def.private = true;
        }
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
        self.hides.clear();
        // Until `begin_module` says otherwise, what follows is a
        // program with no `ns` clauses.
        self.scope = ModuleScope::of("main");
    }

    /// Starts expanding the forms of the module that `begin_module` began:
    /// the names they define (`own::defined_by`) and the names the program's
    /// own modules it `:use`s export hide the prelude macros of the same
    /// name (`own`); the library's modules do not.
    pub(crate) fn hide_macros(&mut self, own: HashSet<String>) {
        let used: Vec<String> = (self.scope.uses.iter())
            .filter(|u| !is_library(u))
            .flat_map(|u| self.exported_names(u))
            .collect();
        self.hides = own;
        self.hides.extend(used);
    }

    /// Adds the names a form that a macro produced defines.
    pub(crate) fn add_own<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        self.hides.extend(names.into_iter().map(str::to_string));
    }

    /// Whether the module being expanded has its own definition of the
    /// bare `name`, which a prelude macro of that name yields to: defined
    /// by the module, or exported by a program module it `:use`s.
    pub(crate) fn hides_macro(&self, name: &str) -> bool {
        self.hides.contains(name)
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
