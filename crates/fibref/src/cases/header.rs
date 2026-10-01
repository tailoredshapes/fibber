//! Case headers: the `;; key: value` lines at the top of a case file that
//! fix its expected verdict (`spec/method.md`, rule 3;
//! `cases/ownership/README.md`).
//!
//! The header is the longest run of lines of the form `;; key: value`
//! starting at line 1. It ends at the first line that is not of that
//! form: a blank line, a comment without a key, or code. Any later
//! `;; ...` line is an ordinary comment, whatever it looks like.
//!
//! Keys: `spec` (non-empty free text, required); `expect` (`accept`,
//! `reject` or `trap`, required); `result` (an integer, required for
//! `accept`, forbidden otherwise); `audit` (`clean` or `leak-cycle`,
//! same); `error` (non-empty text the compile error must contain,
//! required for `reject`, forbidden otherwise); `trap` (non-empty text
//! the run-time trap must contain, required for `trap`, forbidden
//! otherwise); `allocs` (`<= N`, `N` a non-negative integer; optional,
//! `accept` only): the most heap objects the run may allocate, counted
//! as the `A` lines of the free trace (`spec/compiler.md` §4); `roots`
//! (optional, any verdict): library directories, separated by white
//! space and relative to the case file's directory, that the case is run
//! with as `-I` flags are (`cases/modules/README.md`, [`case_roots`]).
//! Anything else is a [`HeaderError`] naming the file and line; parsing
//! never panics.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// The keys a header may contain, in the order the README lists them.
const KEYS: [&str; 8] = [
    "spec", "expect", "result", "audit", "allocs", "roots", "error", "trap",
];

/// The value `main` is expected to return. Only integers exist today;
/// other kinds are added here as the language grows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected {
    /// A signed 64-bit integer.
    Int(i64),
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expected::Int(n) => write!(f, "{n}"),
        }
    }
}

/// What the memory audit must report for an `accept` case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditExpect {
    /// Nothing live at exit, no errors.
    Clean,
    /// Only leaks that are cycles through cells (`spec/ownership.md` §6).
    LeakCycle,
}

impl fmt::Display for AuditExpect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AuditExpect::Clean => "clean",
            AuditExpect::LeakCycle => "leak-cycle",
        })
    }
}

/// The verdict a case header fixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Must compile, run, return `result` and finish with `audit`,
    /// allocating at most `allocs` heap objects when that is given.
    Accept {
        result: Expected,
        audit: AuditExpect,
        allocs: Option<u64>,
    },
    /// Must fail to compile with an error containing `error`.
    Reject { error: String },
    /// Must compile and then trap at run time with a message containing
    /// `trap` (spec/method.md rule 3; types §2.12).
    Trap { trap: String },
}

/// A parsed case header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The spec section that decides the case; free text.
    pub spec: String,
    /// The expected verdict.
    pub verdict: Verdict,
}

/// Why a header could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderErrorKind {
    /// The file could not be read.
    Unreadable(String),
    /// Line 1 is not a `;; key: value` line.
    Empty,
    /// A key that is not one of [`KEYS`].
    UnknownKey(String),
    /// A key given twice.
    DuplicateKey(String),
    /// A key the verdict requires is absent.
    MissingKey(&'static str),
    /// A key the verdict forbids is present.
    ForbiddenKey(&'static str),
    /// A value that does not parse for its key.
    BadValue { key: &'static str, value: String },
}

impl fmt::Display for HeaderErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderErrorKind::Unreadable(why) => write!(f, "cannot read case: {why}"),
            HeaderErrorKind::Empty => write!(f, "empty header: line 1 is not `;; key: value`"),
            HeaderErrorKind::UnknownKey(k) => write!(f, "unknown header key `{k}`"),
            HeaderErrorKind::DuplicateKey(k) => write!(f, "duplicate header key `{k}`"),
            HeaderErrorKind::MissingKey(k) => write!(f, "missing required header key `{k}`"),
            HeaderErrorKind::ForbiddenKey(k) => {
                write!(f, "header key `{k}` is not allowed for this verdict")
            }
            HeaderErrorKind::BadValue { key, value } => {
                write!(f, "bad value `{value}` for header key `{key}`")
            }
        }
    }
}

/// A header that could not be parsed: where, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderError {
    /// The case file.
    pub path: PathBuf,
    /// 1-based line the error is reported at; 0 when the file was unreadable.
    pub line: usize,
    /// What went wrong.
    pub kind: HeaderErrorKind,
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.path.display(), self.line, self.kind)
    }
}

impl Error for HeaderError {}

/// One `;; key: value` line, with its line number.
#[derive(Debug)]
struct Field<'a> {
    key: &'static str,
    value: &'a str,
    line: usize,
}

/// The raw header lines before validation.
struct Fields<'a> {
    path: &'a Path,
    fields: Vec<Field<'a>>,
    /// The line number of the last header line, where missing keys are reported.
    end: usize,
}

/// Splits a line of the form `;; key: value` into its key and trimmed
/// value. Returns `None` for any other line, which ends the header.
fn header_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix(";;")?.trim_start();
    let (key, value) = rest.split_once(':')?;
    let is_key_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    if key.is_empty() || !key.chars().all(is_key_char) {
        return None;
    }
    Some((key, value.trim()))
}

/// Collects the header lines from the top of `source`, rejecting unknown
/// and duplicate keys as they are met.
fn collect<'a>(path: &'a Path, source: &'a str) -> Result<Fields<'a>, HeaderError> {
    let fail = |line, kind| {
        Err(HeaderError {
            path: path.to_path_buf(),
            line,
            kind,
        })
    };
    let mut fields: Vec<Field<'a>> = Vec::new();
    let mut end = 0;
    for (index, text) in source.lines().enumerate() {
        let line = index + 1;
        let Some((key, value)) = header_line(text) else {
            break;
        };
        let Some(&key) = KEYS.iter().find(|k| **k == key) else {
            return fail(line, HeaderErrorKind::UnknownKey(key.to_string()));
        };
        if fields.iter().any(|f| f.key == key) {
            return fail(line, HeaderErrorKind::DuplicateKey(key.to_string()));
        }
        fields.push(Field { key, value, line });
        end = line;
    }
    if fields.is_empty() {
        return fail(1, HeaderErrorKind::Empty);
    }
    Ok(Fields { path, fields, end })
}

/// The maximum in an `allocs` value: `<=`, then a non-negative integer
/// written in decimal digits (so no sign, no `<` or `=`, nothing after).
fn allocs_maximum(value: &str) -> Option<u64> {
    let digits = value.strip_prefix("<=")?.trim_start();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

impl<'a> Fields<'a> {
    fn error(&self, line: usize, kind: HeaderErrorKind) -> HeaderError {
        HeaderError {
            path: self.path.to_path_buf(),
            line,
            kind,
        }
    }

    fn get(&self, key: &'static str) -> Option<&Field<'a>> {
        self.fields.iter().find(|f| f.key == key)
    }

    fn required(&self, key: &'static str) -> Result<&Field<'a>, HeaderError> {
        self.get(key)
            .ok_or_else(|| self.error(self.end, HeaderErrorKind::MissingKey(key)))
    }

    fn forbidden(&self, key: &'static str) -> Result<(), HeaderError> {
        match self.get(key) {
            Some(field) => Err(self.error(field.line, HeaderErrorKind::ForbiddenKey(key))),
            None => Ok(()),
        }
    }

    /// Fetches a required key and parses its value with `parse`.
    fn parsed<T>(
        &self,
        key: &'static str,
        parse: impl FnOnce(&str) -> Option<T>,
    ) -> Result<T, HeaderError> {
        let field = self.required(key)?;
        parse(field.value).ok_or_else(|| {
            self.error(
                field.line,
                HeaderErrorKind::BadValue {
                    key,
                    value: field.value.to_string(),
                },
            )
        })
    }

    fn accept(&self) -> Result<Verdict, HeaderError> {
        let result = self.parsed("result", |v| v.parse().ok().map(Expected::Int))?;
        let audit = self.parsed("audit", |v| match v {
            "clean" => Some(AuditExpect::Clean),
            "leak-cycle" => Some(AuditExpect::LeakCycle),
            _ => None,
        })?;
        let allocs = match self.get("allocs") {
            Some(_) => Some(self.parsed("allocs", allocs_maximum)?),
            None => None,
        };
        self.forbidden("error")?;
        self.forbidden("trap")?;
        Ok(Verdict::Accept {
            result,
            audit,
            allocs,
        })
    }

    fn reject(&self) -> Result<Verdict, HeaderError> {
        let error = self.parsed("error", |v| (!v.is_empty()).then(|| v.to_string()))?;
        self.forbidden("result")?;
        self.forbidden("audit")?;
        self.forbidden("allocs")?;
        self.forbidden("trap")?;
        Ok(Verdict::Reject { error })
    }

    fn trap(&self) -> Result<Verdict, HeaderError> {
        let trap = self.parsed("trap", |v| (!v.is_empty()).then(|| v.to_string()))?;
        self.forbidden("result")?;
        self.forbidden("audit")?;
        self.forbidden("allocs")?;
        self.forbidden("error")?;
        Ok(Verdict::Trap { trap })
    }

    fn into_header(self) -> Result<Header, HeaderError> {
        // `spec` names the section that decides the case; an empty one
        // names nothing, so it is a bad value like an empty `error`.
        let spec = self.parsed("spec", |v| (!v.is_empty()).then(|| v.to_string()))?;
        let expect = self.parsed("expect", |v| {
            ["accept", "reject", "trap"].into_iter().find(|k| *k == v)
        })?;
        let verdict = match expect {
            "accept" => self.accept()?,
            "reject" => self.reject()?,
            _ => self.trap()?,
        };
        Ok(Header { spec, verdict })
    }
}

/// Parses the header at the top of `source`, which was read from `path`.
pub fn parse_header(path: &Path, source: &str) -> Result<Header, HeaderError> {
    collect(path, source)?.into_header()
}

/// Reads and parses the header of the case file at `path`.
pub fn read_header(path: &Path) -> Result<Header, HeaderError> {
    let source = std::fs::read_to_string(path).map_err(|e| HeaderError {
        path: path.to_path_buf(),
        line: 0,
        kind: HeaderErrorKind::Unreadable(e.to_string()),
    })?;
    parse_header(path, &source)
}

/// The library directories the header of `source` (read from `path`)
/// names with `roots`, in order, each joined to the directory of `path`
/// (an absolute one stays as it is); none when the key is absent. These
/// are the roots both harnesses run the case with, `-I` for `-I`
/// (spec/compiler.md §1). A key with no directory is a bad value.
pub fn case_roots(path: &Path, source: &str) -> Result<Vec<PathBuf>, HeaderError> {
    let fields = collect(path, source)?;
    let Some(field) = fields.get("roots") else {
        return Ok(Vec::new());
    };
    let base = path.parent().unwrap_or(Path::new(""));
    let dirs: Vec<PathBuf> = field
        .value
        .split_whitespace()
        .map(|d| base.join(d))
        .collect();
    if dirs.is_empty() {
        let value = field.value.to_string();
        let kind = HeaderErrorKind::BadValue {
            key: "roots",
            value,
        };
        return Err(fields.error(field.line, kind));
    }
    Ok(dirs)
}

#[cfg(test)]
mod tests;
