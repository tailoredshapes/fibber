//! `stdlib_table`: the function table of `spec/stdlib.md` §4 made
//! executable (stdlib §8.1 item 1). The page lists every Clojure name the
//! library offers with the tranche that delivers it; a case of
//! `cases/stdlib/` says which rows it calls with the header key `covers`;
//! this test reads both and fails when they disagree:
//!
//! * a row of a delivered tranche is covered by no case (`every_row_of_the_tranche_is_covered`,
//!   `#[ignore]`d until tranche 1 is complete, see below);
//! * a case covers a name that is no row, covers a row whose spelling its
//!   code never calls (a `covers:` line alone cannot fail), or covers more
//!   than 12 names (a list that long tests nothing in particular);
//! * two rows of one section have one name;
//! * a case is named or labelled against its own header (`open-` without
//!   `open:`, `count-` without `allocs`);
//! * an `open:` item is outside the list of items that may still be open
//!   (`open_cases_are_allowed`).
//!
//! The first check is `#[ignore]`d while the tranche is being written: every
//! package of the plan delivers its own rows, so the check cannot pass
//! before the last one lands, and a red test in every crate run would hide
//! the failures that are news. `cargo test -p fibref --test stdlib_table --
//! --ignored` runs it and lists the rows still uncovered, by section; the
//! final gate of the tranche runs it and removes the `#[ignore]` with
//! `TRANCHE` raised to the next tranche. The other checks always run.
//!
//! The pieces: `rows` reads the table, `cases` reads the case files and
//! the §7 items they may wait for, `cover` compares the two. Each carries
//! unit tests over fixtures, including the three canaries (a row with no
//! case, a `covers:` of a missing name, a case that lists a name and never
//! calls it) shown failing.

mod stdlib_table {
    mod cases;
    mod cover;
    mod rows;

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

    /// The row of every case that covers it, printed so that a package can
    /// see its rows; the count is what the tranche's gate reads.
    #[test]
    #[ignore = "tranche 1 is not complete: run with --ignored; the final gate removes this"]
    fn every_row_of_the_tranche_is_covered() {
        let rows = table();
        let cases = cases();
        let missing = cover::uncovered(&rows, &cases, TRANCHE);
        let total = rows.iter().filter(|r| r.tranche <= TRANCHE).count();
        eprintln!("{}", cover::per_case(&cases));
        assert!(
            missing.is_empty(),
            "{} of {total} rows of tranche {TRANCHE} are covered by no case:\n{}",
            missing.len(),
            cover::by_section(&missing)
        );
    }
}
