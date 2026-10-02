//! The cases of a run: what a directory holds as cases, and the patterns of
//! `--only` that choose among them.

use std::io;
use std::path::Path;

/// A case of a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// What patterns match: the file name without `.fib`, or the name of the
    /// directory of a program of several modules.
    pub key: String,
    /// What the tool's own `--only` is given to run exactly this case: the
    /// file name (a prefix of no other case's), or the directory name.
    pub arg: String,
}

/// The cases the harness would run in `dir`: the `*.fib` files directly in
/// it and the subdirectories that have a `main.fib` (`cases/modules/README.md`),
/// by key. A directory without a `main.fib` (`support`) holds modules, not a case.
pub fn list(dir: &Path) -> io::Result<Vec<Case>> {
    let mut cases = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if path.is_dir() {
            if path.join("main.fib").is_file() {
                cases.push(Case {
                    key: name.clone(),
                    arg: name,
                });
            }
        } else if let Some(stem) = name.strip_suffix(".fib") {
            cases.push(Case {
                key: stem.to_string(),
                arg: name.clone(),
            });
        }
    }
    cases.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(cases)
}

/// Whether `pattern` matches `text`: `*` is any run of characters, `?` one
/// character; a pattern with neither is a prefix, as the tools' `--only` is.
pub fn glob(pattern: &str, text: &str) -> bool {
    if !pattern.contains(['*', '?']) {
        return text.starts_with(pattern);
    }
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    let mut resume: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) && p[pi] != '*' {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            resume = Some((pi, ti));
            pi += 1;
        } else if let Some((star, at)) = resume {
            resume = Some((star, at + 1));
            pi = star + 1;
            ti = at + 1;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// The cases some pattern matches, in their order; every case when there is
/// no pattern. A pattern that matches no case is an error, never an empty run.
pub fn select(cases: Vec<Case>, patterns: &[String]) -> Result<Vec<Case>, String> {
    if patterns.is_empty() {
        return Ok(cases);
    }
    let hits = |p: &String, c: &Case| glob(p, &c.key) || glob(p, &c.arg);
    if let Some(p) = patterns.iter().find(|p| !cases.iter().any(|c| hits(p, c))) {
        return Err(format!("no case matches {p}"));
    }
    Ok(cases
        .into_iter()
        .filter(|c| patterns.iter().any(|p| hits(p, c)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::TempDir;
    use std::fs;

    fn case(key: &str) -> Case {
        Case {
            key: key.into(),
            arg: format!("{key}.fib"),
        }
    }

    #[test]
    fn a_pattern_without_a_wildcard_is_a_prefix() {
        assert!(glob("240-", "240-map"));
        assert!(!glob("240-", "1240-map"));
        assert!(glob("", "x"));
    }

    #[test]
    fn stars_and_marks_match_runs_and_characters() {
        assert!(glob("240-*", "240-map-over-a-vec"));
        assert!(!glob("240-*", "241-map"));
        assert!(glob("*-ref-*", "070-ref-mod"));
        assert!(!glob("*-ref-*", "070-law-mod"));
        assert!(glob("2?0-*", "250-x"));
        assert!(glob("*", ""));
        assert!(glob("a*b*c", "aXXbYYc"));
        assert!(!glob("a*b*c", "aXXbYY"));
        assert!(glob("a*", "a"));
        assert!(!glob("a?", "a"));
    }

    #[test]
    fn the_listing_is_the_harnesss_idea_of_a_case() {
        let dir = TempDir::new("fibmut-t").unwrap();
        let p = dir.path();
        fs::write(p.join("002-b.fib"), "").unwrap();
        fs::write(p.join("001-a.fib"), "").unwrap();
        fs::write(p.join("README.md"), "").unwrap();
        fs::create_dir_all(p.join("003-prog")).unwrap();
        fs::write(p.join("003-prog/main.fib"), "").unwrap();
        fs::create_dir_all(p.join("support/tl")).unwrap();
        fs::write(p.join("support/tl/x.fib"), "").unwrap();
        let got = list(p).unwrap();
        let keys: Vec<(&str, &str)> = got
            .iter()
            .map(|c| (c.key.as_str(), c.arg.as_str()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("001-a", "001-a.fib"),
                ("002-b", "002-b.fib"),
                ("003-prog", "003-prog")
            ]
        );
    }

    #[test]
    fn selection_keeps_the_order_and_refuses_a_pattern_that_matches_nothing() {
        let all = vec![case("001-a"), case("002-b"), case("ref-c")];
        let pick = |ps: &[&str]| {
            let ps: Vec<String> = ps.iter().map(|s| s.to_string()).collect();
            select(all.clone(), &ps).map(|v| v.into_iter().map(|c| c.key).collect::<Vec<_>>())
        };
        assert_eq!(pick(&[]).unwrap().len(), 3);
        assert_eq!(pick(&["002-*", "001-"]).unwrap(), vec!["001-a", "002-b"]);
        assert_eq!(pick(&["ref-c.fib"]).unwrap(), vec!["ref-c"]);
        assert_eq!(pick(&["001-", "999-"]).unwrap_err(), "no case matches 999-");
    }
}
