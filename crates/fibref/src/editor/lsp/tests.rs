use std::io::Cursor;

use super::*;

const URI: &str = "file:///nonexistent/dir/a%20b.fib";

fn request(id: usize, method: &str, params: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", Json::int(id)),
        ("method", Json::str(method)),
        ("params", params),
    ])
}

fn note(method: &str, params: Json) -> Json {
    notification(method, params)
}

fn open(text: &str) -> Json {
    note(
        "textDocument/didOpen",
        Json::obj([(
            "textDocument",
            Json::obj([("uri", Json::str(URI)), ("text", Json::str(text))]),
        )]),
    )
}

fn at(line: usize, character: usize) -> Json {
    Json::obj([
        ("textDocument", Json::obj([("uri", Json::str(URI))])),
        (
            "position",
            Json::obj([
                ("line", Json::int(line)),
                ("character", Json::int(character)),
            ]),
        ),
    ])
}

/// Sends `msgs` framed through `serve` and returns what came back and
/// the exit code.
fn session(msgs: &[Json]) -> (Vec<Json>, i32) {
    let mut input = Vec::new();
    for m in msgs {
        write_message(&mut input, &m.to_text()).expect("frames");
    }
    let mut out = Vec::new();
    let code = serve(
        &mut Server::new(Roots::default()),
        &mut Cursor::new(input),
        &mut out,
    );
    let mut r = Cursor::new(out);
    let mut replies = Vec::new();
    while let Some(body) = read_message(&mut r).expect("framed output") {
        replies.push(parse(&body).expect("JSON output"));
    }
    (replies, code)
}

fn result_of(replies: &[Json], id: usize) -> &Json {
    let r = replies
        .iter()
        .find(|r| r.get("id").and_then(Json::as_usize) == Some(id));
    r.and_then(|r| r.get("result")).expect("a result")
}

fn labels(items: &Json) -> Vec<&str> {
    items
        .as_arr()
        .expect("an array")
        .iter()
        .filter_map(|i| i.get("label")?.as_str())
        .collect()
}

fn diagnostics_of(replies: &[Json]) -> Vec<&Json> {
    replies
        .iter()
        .filter(|r| {
            r.get("method").and_then(Json::as_str) == Some("textDocument/publishDiagnostics")
        })
        .collect()
}

#[test]
fn initialize_advertises_completion_hover_and_full_sync_and_shutdown_exits_zero() {
    let (replies, code) = session(&[
        request(1, "initialize", Json::obj([])),
        note("initialized", Json::obj([])),
        request(2, "shutdown", Json::Null),
        note("exit", Json::Null),
    ]);
    let caps = result_of(&replies, 1)
        .get("capabilities")
        .expect("capabilities");
    assert_eq!(caps.get("textDocumentSync"), Some(&Json::int(1)));
    assert_eq!(caps.get("hoverProvider"), Some(&Json::Bool(true)));
    let triggers = caps
        .path(&["completionProvider", "triggerCharacters"])
        .and_then(Json::as_arr);
    assert_eq!(triggers.map(<[Json]>::len), Some(5));
    assert_eq!(result_of(&replies, 2), &Json::Null);
    assert_eq!(code, 0);
}

#[test]
fn exit_without_shutdown_is_status_one() {
    assert_eq!(session(&[note("exit", Json::Null)]).1, 1);
}

#[test]
fn a_broken_buffer_gets_one_diagnostic_and_still_completes() {
    let (replies, _) = session(&[
        open("(defun f (x: i64) -> i64 (let ((y 1)) (+ x \n"),
        request(1, "textDocument/completion", at(0, 100)),
    ]);
    let diags = diagnostics_of(&replies);
    assert_eq!(diags.len(), 1);
    let list = diags[0]
        .path(&["params", "diagnostics"])
        .and_then(Json::as_arr)
        .expect("a list");
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].path(&["range", "start", "line"]),
        Some(&Json::int(0))
    );
    assert_eq!(list[0].get("severity"), Some(&Json::int(1)));
    let items = result_of(&replies, 1);
    assert!(
        labels(items).contains(&"x") && labels(items).contains(&"y"),
        "{:?}",
        labels(items)
    );
    assert!(labels(items).contains(&"map"));
}

#[test]
fn a_good_buffer_publishes_no_diagnostics_and_a_change_replaces_the_text() {
    let change = note(
        "textDocument/didChange",
        Json::obj([
            ("textDocument", Json::obj([("uri", Json::str(URI))])),
            (
                "contentChanges",
                Json::Arr(vec![Json::obj([(
                    "text",
                    Json::str("(defun g () -> i64 \"s\")\n"),
                )])]),
            ),
        ]),
    );
    let (replies, _) = session(&[open("(defun f () -> i64 1)\n"), change]);
    let diags = diagnostics_of(&replies);
    assert_eq!(diags.len(), 2);
    assert_eq!(
        diags[0].path(&["params", "diagnostics"]),
        Some(&Json::Arr(vec![]))
    );
    let second = diags[1]
        .path(&["params", "diagnostics"])
        .and_then(Json::as_arr)
        .expect("a list");
    assert_eq!(second.len(), 1, "a type error after the change");
}

#[test]
fn hover_answers_with_the_type_and_null_where_there_is_no_name() {
    let (replies, _) = session(&[
        open("(defun inc (k: i64) -> i64 (+ k 1))\n(defun f () -> i64 (inc 1))\n"),
        request(1, "textDocument/hover", at(1, 20)),
        request(2, "textDocument/hover", at(1, 0)),
    ]);
    let value = result_of(&replies, 1)
        .path(&["contents", "value"])
        .and_then(Json::as_str);
    assert!(
        value.is_some_and(|v| v.contains("inc : (fn") && v.contains("i64")),
        "{value:?}"
    );
    assert_eq!(result_of(&replies, 2), &Json::Null);
}

#[test]
fn bad_requests_are_errors_and_the_server_goes_on() {
    let (replies, _) = session(&[
        request(1, "textDocument/nonsense", Json::obj([])),
        request(2, "textDocument/completion", Json::obj([])),
        request(3, "textDocument/completion", at(0, 0)),
        request(4, "shutdown", Json::Null),
        request(5, "textDocument/hover", at(0, 0)),
    ]);
    let code_of = |id: usize| {
        let r = replies
            .iter()
            .find(|r| r.get("id").and_then(Json::as_usize) == Some(id));
        r.and_then(|r| r.path(&["error", "code"])).cloned()
    };
    assert_eq!(code_of(1), Some(Json::Num(-32601.0)));
    assert_eq!(code_of(2), Some(Json::Num(-32602.0)));
    assert_eq!(result_of(&replies, 3), &Json::Null, "a buffer never opened");
    assert_eq!(code_of(5), Some(Json::Num(-32600.0)), "after shutdown");
}

#[test]
fn malformed_json_is_a_parse_error_reply_not_a_stop() {
    let mut input = Vec::new();
    write_message(&mut input, "{not json").expect("frames");
    write_message(&mut input, &request(1, "shutdown", Json::Null).to_text()).expect("frames");
    write_message(&mut input, &note("exit", Json::Null).to_text()).expect("frames");
    let mut out = Vec::new();
    let code = serve(
        &mut Server::new(Roots::default()),
        &mut Cursor::new(input),
        &mut out,
    );
    let text = String::from_utf8(out).expect("UTF-8");
    assert!(text.contains("-32700"), "{text}");
    assert!(text.contains("\"id\":1"), "{text}");
    assert_eq!(code, 0);
}

#[test]
fn uris_become_paths_with_escapes_decoded() {
    assert_eq!(path_of(URI), "/nonexistent/dir/a b.fib");
    assert_eq!(path_of("untitled:1"), "untitled:1");
}
