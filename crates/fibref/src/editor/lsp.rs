//! `fibref lsp`: a language server over stdio (JSON-RPC 2.0 with
//! Content-Length framing). Full-text synchronisation, completion,
//! hover and diagnostics, each a call of the same code as `fibref
//! complete` and `fibref diagnostics`. A broken buffer is an ordinary
//! input; a handler that panics answers its request with an error and
//! the server goes on.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

use super::analysis::Diag;
use super::catalog::Item;
use super::complete::{complete, hover};
use super::diagnostics::{diagnostics, range};
use super::transport::{read_message, write_message};
use crate::json::{parse, Json};
use crate::roots::Roots;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;

/// The server's state: the open buffers, by URI.
pub struct Server {
    docs: HashMap<String, String>,
    roots: Roots,
    shutdown: bool,
}

/// What a message produced: messages to send, and an exit code when the
/// server must stop.
pub struct Reply {
    pub out: Vec<Json>,
    pub exit: Option<i32>,
}

fn response(id: &Json, result: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", id.clone()),
        ("result", result),
    ])
}

fn failure(id: &Json, code: i64, message: &str) -> Json {
    let error = Json::obj([
        ("code", Json::Num(code as f64)),
        ("message", Json::str(message)),
    ]);
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", id.clone()),
        ("error", error),
    ])
}

fn notification(method: &str, params: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("method", Json::str(method)),
        ("params", params),
    ])
}

/// The file a `file:` URI names (percent escapes decoded); any other
/// URI stands for itself.
pub fn path_of(uri: &str) -> String {
    let Some(rest) = uri.strip_prefix("file://") else {
        return uri.to_string();
    };
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let b = rest.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let hex = |k: usize| b.get(k).and_then(|c| (*c as char).to_digit(16));
        match (b[i], hex(i + 1), hex(i + 2)) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, _, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The LSP `CompletionItemKind` of an item kind of the protocol.
pub fn completion_kind(kind: &str) -> usize {
    match kind {
        "method" => 2,
        "function" => 3,
        "field" => 5,
        "variable" => 6,
        "protocol" => 8,
        "module" => 9,
        "enum" => 13,
        "macro" => 14,
        "variant" => 20,
        "struct" => 22,
        "type" => 25,
        _ => 12,
    }
}

fn lsp_item(i: &Item) -> Json {
    Json::obj([
        ("label", Json::str(&i.label)),
        ("kind", Json::int(completion_kind(i.kind))),
        ("detail", Json::str(&i.detail)),
        ("documentation", Json::str(&i.doc)),
    ])
}

fn position(params: &Json) -> Option<(usize, usize)> {
    let p = params.get("position")?;
    Some((
        p.get("line")?.as_usize()? + 1,
        p.get("character")?.as_usize()?,
    ))
}

impl Server {
    /// A server that finds modules through `roots`.
    pub fn new(roots: Roots) -> Server {
        Server {
            docs: HashMap::new(),
            roots,
            shutdown: false,
        }
    }

    /// Handles one message from the client.
    pub fn handle(&mut self, msg: &Json) -> Reply {
        let id = msg.get("id");
        let result = catch_unwind(AssertUnwindSafe(|| self.dispatch(msg)));
        match (result, id) {
            (Ok(reply), _) => reply,
            (Err(_), Some(id)) => Reply {
                out: vec![failure(
                    id,
                    INTERNAL_ERROR,
                    "internal error while handling the request",
                )],
                exit: None,
            },
            (Err(_), None) => Reply {
                out: Vec::new(),
                exit: None,
            },
        }
    }

    fn dispatch(&mut self, msg: &Json) -> Reply {
        let method = msg.get("method").and_then(Json::as_str).unwrap_or("");
        let null = Json::Null;
        let params = msg.get("params").unwrap_or(&null);
        let Some(id) = msg.get("id") else {
            return self.notification(method, params);
        };
        if method.is_empty() {
            return Reply::none(); // a response to a request we never sent
        }
        if self.shutdown && method != "shutdown" {
            return Reply::one(failure(id, INVALID_REQUEST, "the server is shut down"));
        }
        let out = match self.request(method, params) {
            Ok(result) => response(id, result),
            Err((code, message)) => failure(id, code, &message),
        };
        Reply::one(out)
    }

    fn request(&mut self, method: &str, params: &Json) -> Result<Json, (i64, String)> {
        match method {
            "initialize" => Ok(initialize_result()),
            "shutdown" => {
                self.shutdown = true;
                Ok(Json::Null)
            }
            "textDocument/completion" => self.with_buffer(params, |s, uri, line, col, roots| {
                let items = complete(s, &path_of(uri), line, col, roots);
                Json::Arr(items.iter().map(lsp_item).collect())
            }),
            "textDocument/hover" => {
                self.with_buffer(params, |s, uri, line, col, roots| {
                    match hover(s, &path_of(uri), line, col, roots) {
                        Some(text) => hover_json(&text),
                        None => Json::Null,
                    }
                })
            }
            _ => Err((METHOD_NOT_FOUND, format!("unsupported method {method}"))),
        }
    }

    fn with_buffer(
        &self,
        params: &Json,
        f: impl Fn(&str, &str, usize, usize, &Roots) -> Json,
    ) -> Result<Json, (i64, String)> {
        let uri = params.path(&["textDocument", "uri"]).and_then(Json::as_str);
        let (Some(uri), Some((line, col))) = (uri, position(params)) else {
            return Err((
                INVALID_PARAMS,
                "expected textDocument.uri and position".into(),
            ));
        };
        match self.docs.get(uri) {
            Some(text) => Ok(f(text, uri, line, col, &self.roots)),
            None => Ok(Json::Null),
        }
    }

    fn notification(&mut self, method: &str, params: &Json) -> Reply {
        let doc = params.get("textDocument");
        let uri = doc.and_then(|d| d.get("uri")).and_then(Json::as_str);
        match (method, uri) {
            ("exit", _) => Reply {
                out: Vec::new(),
                exit: Some(if self.shutdown { 0 } else { 1 }),
            },
            ("textDocument/didOpen", Some(uri)) => {
                let text = doc.and_then(|d| d.get("text")).and_then(Json::as_str);
                self.changed(uri, text.unwrap_or(""))
            }
            ("textDocument/didChange", Some(uri)) => {
                let last = params
                    .get("contentChanges")
                    .and_then(Json::as_arr)
                    .and_then(<[Json]>::last);
                match last.and_then(|c| c.get("text")).and_then(Json::as_str) {
                    Some(text) => self.changed(uri, text),
                    None => Reply::none(),
                }
            }
            ("textDocument/didSave", Some(uri)) if self.docs.contains_key(uri) => {
                let text = self.docs[uri].clone();
                self.changed(uri, &text)
            }
            ("textDocument/didClose", Some(uri)) => {
                self.docs.remove(uri);
                Reply::one(publish(uri, Vec::new()))
            }
            _ => Reply::none(),
        }
    }

    fn changed(&mut self, uri: &str, text: &str) -> Reply {
        self.docs.insert(uri.to_string(), text.to_string());
        let diags = diagnostics(text, &path_of(uri), &self.roots);
        Reply::one(publish(
            uri,
            diags.iter().map(|d| lsp_diag(text, d)).collect(),
        ))
    }
}

impl Reply {
    fn one(m: Json) -> Reply {
        Reply {
            out: vec![m],
            exit: None,
        }
    }

    fn none() -> Reply {
        Reply {
            out: Vec::new(),
            exit: None,
        }
    }
}

fn initialize_result() -> Json {
    let completion = Json::obj([(
        "triggerCharacters",
        Json::Arr(
            ["(", "/", ".", ":", " "]
                .iter()
                .map(|c| Json::str(*c))
                .collect(),
        ),
    )]);
    let capabilities = Json::obj([
        ("textDocumentSync", Json::int(1)),
        ("completionProvider", completion),
        ("hoverProvider", Json::Bool(true)),
    ]);
    let info = Json::obj([("name", Json::str("fibref lsp"))]);
    Json::obj([("capabilities", capabilities), ("serverInfo", info)])
}

fn hover_json(text: &str) -> Json {
    let value = format!("```fibber\n{text}\n```");
    Json::obj([(
        "contents",
        Json::obj([("kind", Json::str("markdown")), ("value", Json::Str(value))]),
    )])
}

fn lsp_diag(src: &str, d: &Diag) -> Json {
    let (l, c, el, ec) = range(src, d);
    let at = |line: usize, col: usize| {
        Json::obj([("line", Json::int(line - 1)), ("character", Json::int(col))])
    };
    Json::obj([
        (
            "range",
            Json::obj([("start", at(l, c)), ("end", at(el, ec))]),
        ),
        ("severity", Json::int(1)),
        ("source", Json::str("fibref")),
        ("message", Json::str(&d.message)),
    ])
}

fn publish(uri: &str, diagnostics: Vec<Json>) -> Json {
    notification(
        "textDocument/publishDiagnostics",
        Json::obj([
            ("uri", Json::str(uri)),
            ("diagnostics", Json::Arr(diagnostics)),
        ]),
    )
}

/// Serves `input` to `output` until `exit` or the end of input; the
/// exit code: 0 after `shutdown`, 1 without it.
pub fn serve<R: BufRead, W: Write>(server: &mut Server, input: &mut R, output: &mut W) -> i32 {
    loop {
        let body = match read_message(input) {
            Ok(Some(body)) => body,
            Ok(None) => return 1,
            Err(_) => return 1,
        };
        let replies = match parse(&body) {
            Ok(msg) => server.handle(&msg),
            Err(e) => Reply::one(failure(&Json::Null, PARSE_ERROR, &e)),
        };
        for m in &replies.out {
            if write_message(output, &m.to_text()).is_err() {
                return 1;
            }
        }
        if let Some(code) = replies.exit {
            return code;
        }
    }
}

#[cfg(test)]
mod tests;
