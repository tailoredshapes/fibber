//! Case headers (spec/lir.md §13).

/// Which paths a case runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paths {
    Both,
    Jit,
    Aot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    /// How `main` ends (an exit status, or one of the signals that may
    /// kill the process) and the standard output lines.
    Accept {
        exit: i32,
        signals: Vec<Signal>,
        out: Vec<String>,
    },
    /// Text the error must contain.
    Reject(String),
}

/// A signal a case expects to be killed by, by its POSIX name. The
/// numbers are those of Linux and macOS on every architecture, which
/// is why a case names the signal and not a status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signal(pub i32);

impl Signal {
    const NAMES: [(&'static str, i32); 6] = [
        ("SIGILL", 4),
        ("SIGTRAP", 5),
        ("SIGABRT", 6),
        ("SIGFPE", 8),
        ("SIGSEGV", 11),
        ("SIGTERM", 15),
    ];

    /// A list of names separated by commas or spaces.
    fn parse_list(text: &str) -> Result<Vec<Signal>, String> {
        text.split([',', ' '])
            .filter(|n| !n.is_empty())
            .map(|name| {
                Self::NAMES
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, sig)| Signal(*sig))
                    .ok_or_else(|| format!("unknown signal: {name}"))
            })
            .collect()
    }
}

/// `SIGILL (4)` for a known signal, `signal 42` otherwise.
pub fn signal_name(sig: i32) -> String {
    match Signal::NAMES.iter().find(|(_, n)| *n == sig) {
        Some((name, _)) => format!("{name} ({sig})"),
        None => format!("signal {sig}"),
    }
}

#[derive(Clone, Debug)]
pub struct Header {
    pub expect: Expect,
    pub ir: Vec<String>,
    pub ir_not: Vec<String>,
    pub paths: Paths,
}

/// Read the `;; key: value` lines at the top of a case.
pub fn parse(src: &str) -> Result<Header, String> {
    let mut expect = None;
    let (mut exit, mut out, mut error) = (0, Vec::new(), None);
    let mut signals = Vec::new();
    let (mut ir, mut ir_not, mut paths) = (Vec::new(), Vec::new(), Paths::Both);
    for line in src.lines().take_while(|l| l.starts_with(";;")) {
        let body = line.trim_start_matches(";;").trim_start();
        let Some((key, value)) = body.split_once(':') else {
            continue;
        };
        let value = value.strip_prefix(' ').unwrap_or(value);
        match key {
            "expect" => expect = Some(value.trim().to_string()),
            "exit" => {
                exit = value
                    .trim()
                    .parse()
                    .map_err(|_| format!("bad exit: {value}"))?
            }
            "signal" => signals.extend(Signal::parse_list(value)?),
            "out" => out.push(value.to_string()),
            "error" => error = Some(value.to_string()),
            "ir" => ir.push(value.to_string()),
            "ir-not" => ir_not.push(value.to_string()),
            "paths" => {
                paths = match value.trim() {
                    "jit" => Paths::Jit,
                    "aot" => Paths::Aot,
                    other => return Err(format!("bad paths: {other}")),
                }
            }
            _ => {}
        }
    }
    let expect = match (expect.as_deref(), error) {
        (Some("accept"), None) if !signals.is_empty() && exit != 0 => {
            return Err("accept case with both exit and signal".into())
        }
        (Some("accept"), None) => Expect::Accept { exit, signals, out },
        (Some("reject"), Some(e)) if !e.trim().is_empty() => Expect::Reject(e),
        (Some("reject"), _) => return Err("reject case without an error text".into()),
        (Some("accept"), Some(_)) => return Err("accept case with an error text".into()),
        (other, _) => return Err(format!("bad or missing expect: {other:?}")),
    };
    Ok(Header {
        expect,
        ir,
        ir_not,
        paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_headers() {
        let h = parse(";; expect: accept\n;; exit: 3\n;; out: a b\n;; out: \n;; ir: fence\n(x)")
            .unwrap();
        assert_eq!(
            h.expect,
            Expect::Accept {
                exit: 3,
                signals: vec![],
                out: vec!["a b".into(), "".into()]
            }
        );
        let s = parse(";; expect: accept\n;; signal: SIGILL, SIGTRAP\n").unwrap();
        assert_eq!(
            s.expect,
            Expect::Accept {
                exit: 0,
                signals: vec![Signal(4), Signal(5)],
                out: vec![]
            }
        );
        assert!(parse(";; expect: accept\n;; signal: SIGFOO\n").is_err());
        assert!(parse(";; expect: accept\n;; exit: 1\n;; signal: SIGILL\n").is_err());
        assert_eq!(signal_name(4), "SIGILL (4)");
        assert_eq!(signal_name(42), "signal 42");
        assert_eq!(h.ir, vec!["fence".to_string()]);
        let r = parse(";; expect: reject\n;; error: add: operand 2\n").unwrap();
        assert_eq!(r.expect, Expect::Reject("add: operand 2".into()));
        assert!(parse(";; expect: reject\n").is_err());
        assert!(parse(";; expect: maybe\n").is_err());
        assert!(parse("(define)").is_err());
    }
}
