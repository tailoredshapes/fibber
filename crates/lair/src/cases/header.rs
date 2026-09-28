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
    /// Exit status and standard output lines.
    Accept { exit: i32, out: Vec<String> },
    /// Text the error must contain.
    Reject(String),
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
        (Some("accept"), None) => Expect::Accept { exit, out },
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
                out: vec!["a b".into(), "".into()]
            }
        );
        assert_eq!(h.ir, vec!["fence".to_string()]);
        let r = parse(";; expect: reject\n;; error: add: operand 2\n").unwrap();
        assert_eq!(r.expect, Expect::Reject("add: operand 2".into()));
        assert!(parse(";; expect: reject\n").is_err());
        assert!(parse(";; expect: maybe\n").is_err());
        assert!(parse("(define)").is_err());
    }
}
