//! The command lines of `fibref complete`, `fibref diagnostics` and
//! `fibref lsp`.

use std::io::{self, Read};

use super::complete::{complete, items_json};
use super::diagnostics::{diagnostics, diagnostics_json};
use super::lsp::{serve, Server};
use crate::roots::Roots;

/// The stack of the thread that serves: the checker needs a deep one
/// (types.rs `CHECK_STACK`) and the expander recurses on the caller's.
const SERVE_STACK: usize = 256 * 1024 * 1024;

/// A command's output and exit status: the document for stdout, or the
/// reason for stderr with status 2.
pub type Outcome = Result<String, String>;

struct Buffer {
    source: String,
    path: String,
}

/// `FILE` (or `-` for standard input) and the optional `--path NAME`.
fn buffer(file: &str, path: Option<&str>) -> Result<Buffer, String> {
    let source = if file == "-" {
        let mut s = String::new();
        io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| format!("fibref: cannot read standard input: {e}"))?;
        s
    } else {
        std::fs::read_to_string(file).map_err(|e| format!("fibref: cannot read {file}: {e}"))?
    };
    let path = path.unwrap_or(if file == "-" { "buffer.fib" } else { file });
    Ok(Buffer {
        source,
        path: path.to_string(),
    })
}

/// Splits `--path NAME` out of `args`.
fn split_path(args: &[String]) -> Result<(Vec<&str>, Option<&str>), String> {
    let mut rest = Vec::new();
    let mut path = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--path" {
            path = Some(it.next().ok_or("--path needs a name")?.as_str());
        } else {
            rest.push(a.as_str());
        }
    }
    Ok((rest, path))
}

/// `fibref complete FILE LINE COL [--path NAME]`.
pub fn complete_cmd(args: &[String], roots: &Roots) -> Outcome {
    let (rest, path) = split_path(args)?;
    let [file, line, col] = rest[..] else {
        return Err("usage: fibref complete [-I dir].. FILE|- LINE COL [--path NAME]".into());
    };
    let num = |s: &str| s.parse::<usize>().map_err(|_| format!("not a number: {s}"));
    let b = buffer(file, path)?;
    let items = complete(&b.source, &b.path, num(line)?, num(col)?, roots);
    Ok(items_json(&items).to_text() + "\n")
}

/// `fibref diagnostics FILE|- [--path NAME]`.
pub fn diagnostics_cmd(args: &[String], roots: &Roots) -> Outcome {
    let (rest, path) = split_path(args)?;
    let [file] = rest[..] else {
        return Err("usage: fibref diagnostics [-I dir].. FILE|- [--path NAME]".into());
    };
    let b = buffer(file, path)?;
    let diags = diagnostics(&b.source, &b.path, roots);
    Ok(diagnostics_json(&b.source, &diags).to_text() + "\n")
}

/// `fibref lsp`: serves standard input and output; the exit status.
pub fn lsp_cmd(roots: Roots) -> i32 {
    let serving = std::thread::Builder::new()
        .name("fibref-lsp".into())
        .stack_size(SERVE_STACK)
        .spawn(move || {
            let mut server = Server::new(roots);
            let stdin = io::stdin();
            serve(&mut server, &mut stdin.lock(), &mut io::stdout().lock())
        });
    match serving.map(|h| h.join()) {
        Ok(Ok(code)) => code,
        _ => 1,
    }
}
