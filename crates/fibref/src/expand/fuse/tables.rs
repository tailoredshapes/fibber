//! The two sets of the fusion rewrite (stdlib design §2.1 rule 2, tranche 1
//! plan R9), as they stand in tranche 1: the names that exist today, the
//! page's sets restricted.
//!
//! **A**, the sequence functions that have a recipe, with the recipe's
//! constructor (the structs of `lib/fib/seq/recipes.fib`, source first).
//! **T**, the terminal consumers, with how many arguments the call has and
//! where its collection is: the last argument, except for `nth`, which
//! reads `(nth c i)`. The page's T also has `str/join` and `seq=`: neither
//! is a name of the library today (`str/join` lives in a module the expander
//! cannot resolve, `seq=` is tranche 2). `first rest next seq doall
//! take-last` and everything that returns a seq are not in T.

/// The implicit module a name is defined in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Facade {
    Seq,
    Coll,
}

impl Facade {
    /// The qualifier of a head written `fib.seq/name` or `fib.coll/name`.
    pub(super) fn qualifier(self) -> &'static str {
        match self {
            Facade::Seq => "fib.seq/",
            Facade::Coll => "fib.coll/",
        }
    }
}

/// A terminal consumer: the call `(name arg ..)` with `args` arguments,
/// whose collection is the first argument (`coll_first`) or the last.
pub(super) struct Terminal {
    pub name: &'static str,
    pub args: usize,
    pub coll_first: bool,
    pub facade: Facade,
}

const fn seq(name: &'static str, args: usize) -> Terminal {
    Terminal {
        name,
        args,
        coll_first: false,
        facade: Facade::Seq,
    }
}

const fn coll(name: &'static str, args: usize) -> Terminal {
    Terminal {
        name,
        args,
        coll_first: false,
        facade: Facade::Coll,
    }
}

/// **T**. A name may have several rows (one per arity that is a consumer).
pub(super) const TERMINALS: &[Terminal] = &[
    seq("reduce", 3),
    seq("reduce1", 2),
    seq("reduce-while", 3),
    seq("reduce-nonempty", 2),
    seq("count", 1),
    seq("empty?", 1),
    seq("last", 1),
    Terminal {
        name: "nth",
        args: 2,
        coll_first: true,
        facade: Facade::Seq,
    },
    coll("into", 2),
    coll("vec", 1),
    coll("set", 1),
    seq("run!", 2),
    seq("every?", 2),
    seq("not-any?", 2),
    seq("find-first", 2),
    seq("find-map", 2),
    seq("group-by", 2),
    seq("frequencies", 1),
    seq("sort", 1),
    seq("sort-by", 2),
    seq("sort-with", 2),
    seq("sort-by-with", 3),
    seq("sum", 1),
];

/// What a stage of a chain becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    /// `(name p c)` is `(Recipe c p)`.
    Param,
    /// `(remove p c)` is `(Filtered c (fn (x) (not (truthy? (p x)))))`.
    Complement,
    /// `(concat a b)` is `(Cat a b)`: two collections.
    Two,
}

/// A member of **A**: the name, the recipe constructor, the shape.
pub(super) struct Stage {
    pub name: &'static str,
    pub recipe: &'static str,
    pub shape: Shape,
}

const fn stage(name: &'static str, recipe: &'static str, shape: Shape) -> Stage {
    Stage {
        name,
        recipe,
        shape,
    }
}

/// **A**: every one has two arguments, and lives in `fib.seq`.
pub(super) const STAGES: &[Stage] = &[
    stage("map", "fib.seq/Mapped", Shape::Param),
    stage("filter", "fib.seq/Filtered", Shape::Param),
    stage("remove", "fib.seq/Filtered", Shape::Complement),
    stage("take", "fib.seq/Taken", Shape::Param),
    stage("drop", "fib.seq/Dropped", Shape::Param),
    stage("take-while", "fib.seq/TakenWhile", Shape::Param),
    stage("mapcat", "fib.seq/Mapcat", Shape::Param),
    stage("concat", "fib.seq/Cat", Shape::Two),
];

/// A head split into the facade it names and the bare name: `fib.seq/map`
/// is `(Some(Seq), "map")`, `map` is `(None, "map")`.
pub(super) fn split_head(head: &str) -> (Option<Facade>, &str) {
    for facade in [Facade::Seq, Facade::Coll] {
        if let Some(base) = head.strip_prefix(facade.qualifier()) {
            return (Some(facade), base);
        }
    }
    (None, head)
}

/// `head` as the library's name, with a clause suffix of the call's own
/// argument count taken off: the expander picked the clause of an
/// overloaded library function (`map` with a 3-argument clause makes
/// `(map f c)` the call `(map$2 f c)`), and the tables key on the base
/// name and the arity of the call. The suffix is split off the name and
/// not the qualifier, so `fib.seq/map$2` is `(Some(Seq), "map")` too.
pub(super) fn base_head(head: &str, argc: usize) -> (Option<Facade>, &str) {
    let suffix = format!("${argc}");
    split_head(head.strip_suffix(suffix.as_str()).unwrap_or(head))
}

/// The row of **T** for a call of `args` arguments by this bare name.
pub(super) fn terminal(name: &str, args: usize) -> Option<&'static Terminal> {
    TERMINALS.iter().find(|t| t.name == name && t.args == args)
}

/// The row of **A** for this bare name.
pub(super) fn stage_named(name: &str) -> Option<&'static Stage> {
    STAGES.iter().find(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_head_names_its_facade_or_none() {
        assert_eq!(split_head("fib.seq/map"), (Some(Facade::Seq), "map"));
        assert_eq!(split_head("fib.coll/vec"), (Some(Facade::Coll), "vec"));
        assert_eq!(split_head("map"), (None, "map"));
        assert_eq!(split_head("fib.core/map"), (None, "fib.core/map"));
    }

    #[test]
    fn a_clause_picked_head_is_its_base_name() {
        assert_eq!(base_head("map$2", 2), (None, "map"));
        assert_eq!(base_head("fib.seq/map$2", 2), (Some(Facade::Seq), "map"));
        assert_eq!(base_head("sort$1", 1), (None, "sort"));
        assert_eq!(base_head("map$3", 2), (None, "map$3"));
        assert_eq!(base_head("map", 2), (None, "map"));
    }

    #[test]
    fn a_terminal_is_found_by_name_and_arity() {
        assert!(terminal("reduce", 3).is_some());
        assert!(terminal("reduce", 2).is_none());
        assert!(terminal("nth", 2).is_some_and(|t| t.coll_first));
        assert!(terminal("first", 1).is_none());
        assert!(terminal("vec", 1).is_some_and(|t| t.facade == Facade::Coll));
    }

    #[test]
    fn the_stages_are_the_adaptors_with_a_recipe() {
        let names: Vec<&str> = STAGES.iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            [
                "map",
                "filter",
                "remove",
                "take",
                "drop",
                "take-while",
                "mapcat",
                "concat"
            ]
        );
        assert!(stage_named("first").is_none());
    }
}
