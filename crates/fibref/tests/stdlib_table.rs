//! `stdlib_table`: the function table of `spec/stdlib.md` §4 made
//! executable (stdlib §8.1 item 1). The page lists every Clojure name the
//! library offers with the tranche that delivers it; a case of
//! `cases/stdlib/` says which rows it calls with the header key `covers`;
//! this test reads both and fails when they disagree:
//!
//! * a row of a delivered tranche is covered by no case
//!   (`every_row_of_the_tranche_is_covered`, for the rows with `T` at or below
//!   `TRANCHE`);
//! * a case covers a name that is no row, covers a row whose spelling its
//!   code never calls (a `covers:` line alone cannot fail), or covers more
//!   than 12 names (a list that long tests nothing in particular);
//! * two rows of one section have one name;
//! * a case is named or labelled against its own header (`open-` without
//!   `open:`, `count-` without `allocs`);
//! * an `open:` item is outside the list of items that may still be open
//!   (`open_cases_are_allowed`).
//!
//! The first check was `#[ignore]`d while tranche 1 was written (every package
//! of the plan delivers its own rows, so it could not pass before the last
//! one landed, and a red test in every crate run would hide the failures
//! that are news). Tranche 2's Z0 ran it with `--ignored` against the tree of
//! the flip, saw it pass for tranche 1, and removed the attribute, so it now
//! always runs. `TRANCHE` stays 1 while tranche 2's packages deliver their
//! rows: the gate of tranche 2 (ZF) raises it to 2, and from then on a row of
//! the table with `T` 2 that no case covers fails the build. Until then
//! `every_row_of_the_next_tranche_is_covered` is `#[ignore]`d and
//! `cargo test -p fibref --test stdlib_table -- --ignored` lists the rows of
//! tranche 2 still uncovered, which is how a package sees its own.
//!
//! The pieces: `rows` reads the table, `cases` reads the case files and
//! the §7 items they may wait for, `cover` compares the two, `syntax` says
//! what calls a row that has no token (`@x`, `#(..)`, `(:k x)`, `->Name`, and
//! `some`, which is also `Option`'s constructor). Each carries unit tests over
//! fixtures, including the canaries (a row with no case, a `covers:` of a
//! missing name, a case that lists a name and never calls it, a case that
//! covers `some` with no call of two operands) shown failing.

mod stdlib_table {
    mod cases;
    mod cover;
    mod rows;
    mod syntax;

    use std::path::Path;

    use cases::{load_cases, Case};
    use rows::{parse_table, Row};

    /// The tranche whose rows must be covered (stdlib §8.2).
    pub const TRANCHE: u8 = 1;
    /// The function table of the spec.
    pub const SPEC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../spec/stdlib.md");
    /// The library's cases.
    pub const CASES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cases/stdlib");

    fn table() -> Vec<Row> {
        let text = std::fs::read_to_string(SPEC).expect("spec/stdlib.md is readable");
        parse_table(&text).unwrap_or_else(|e| panic!("spec/stdlib.md §4: {e}"))
    }

    fn cases() -> Vec<Case> {
        load_cases(Path::new(CASES)).unwrap_or_else(|e| panic!("cases/stdlib: {e}"))
    }

    #[test]
    fn the_table_parses_and_has_the_rows_of_the_tranche() {
        let rows = table();
        assert!(rows.len() > 600, "only {} rows parsed", rows.len());
        let delivered = rows.iter().filter(|r| r.tranche <= TRANCHE).count();
        assert!(
            delivered >= 150,
            "only {delivered} rows of tranche {TRANCHE}"
        );
        assert!(
            cover::duplicate_names(&rows).is_empty(),
            "{:?}",
            cover::duplicate_names(&rows)
        );
    }

    #[test]
    fn every_case_covers_real_rows_it_calls() {
        let findings = cover::claims(&table(), &cases());
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    #[test]
    fn cases_are_named_and_labelled_alike() {
        let findings = cases::naming(&cases());
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    #[test]
    fn open_cases_are_allowed() {
        let findings = cases::open_items_outside(&cases(), cases::ALLOWED_OPEN);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// The rows of the tranche that no case covers fail the build; the names
    /// each case covers are printed so that a package can see its rows.
    #[test]
    fn every_row_of_the_tranche_is_covered() {
        assert_covered(TRANCHE);
    }

    /// The same check for the tranche being written: `-- --ignored` lists the
    /// rows of tranche 2 that no case covers yet, by section, and fails until
    /// the last package delivers; the gate of tranche 2 raises `TRANCHE` and
    /// removes this test. It is the way a package of the tranche sees whether
    /// its own rows are covered.
    #[test]
    #[ignore = "tranche 2 is being written: run with --ignored; its gate raises TRANCHE and removes this"]
    fn every_row_of_the_next_tranche_is_covered() {
        assert_covered(TRANCHE + 1);
    }

    fn assert_covered(tranche: u8) {
        let rows = table();
        let cases = cases();
        let missing = cover::uncovered(&rows, &cases, tranche);
        let total = rows.iter().filter(|r| r.tranche <= tranche).count();
        eprintln!("{}", cover::per_case(&cases));
        assert!(
            missing.is_empty(),
            "{} of {total} rows of tranche {tranche} are covered by no case:\n{}",
            missing.len(),
            cover::by_section(&missing)
        );
    }
}
