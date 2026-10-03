//! The oracle of the macro tests: the Rust `JitRunner` expands a program
//! and every run of a macro is recorded (the call, the gensym counter on
//! either side, what reflection answers, the result and the
//! module's lIR text), then `jit-demo macro` runs the same call from
//! fibber on that text and its output is compared with the record.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Output;

use fibc::macros::JitRunner;
use fibref::expand::{
    expand_program, ExpandCtx, ExpandError, ExpandErrorKind, MacroDef, MacroRunner,
    REFLECTION_CALLS,
};
use fibref::syntax::{read_all, Form, FormKind, Pos};
use fibref::types::prelude_forms;

use super::support::{clean_stdout, run_demo, shared_dir};

/// One run of a macro, with what the Rust runner made of it.
pub struct Run {
    pub def: MacroDef,
    /// The index of the macro's module: its functions are `fibm.NAME.K`;
    /// `None` when the module could not be made (the macro is refused).
    pub k: Option<usize>,
    /// The call as text, `(name arg ..)`, each argument as `Display` prints it.
    pub call: String,
    /// The gensym counter before the call and after it.
    pub start: u64,
    pub after: u64,
    /// What reflection says of every type name the macro could ask about,
    /// as the text `(OP NAME FORM) ..` (`(OP NAME :error "MESSAGE")` for an
    /// error) the demo reads: the expander's own table.
    pub answers: String,
    pub result: Result<Form, ExpandError>,
}

/// The number of a gensym `#prefix.N`.
fn counter(sym: &Form) -> u64 {
    let FormKind::Sym(name) = &sym.kind else {
        panic!("a gensym is a Sym: {sym:?}");
    };
    name.rsplit('.')
        .next()
        .and_then(|n| n.parse().ok())
        .expect("#prefix.N")
}

/// The names `defstruct` and `defenum` define in `source`, found by
/// reading for `(defstruct Name` and `(defenum (Name a) ..`, and the two
/// built-in enums reflection knows.
fn type_names(source: &str) -> Vec<String> {
    let mut names = vec!["Option".to_string(), "Form".to_string()];
    for head in ["(defstruct", "(defenum"] {
        for rest in source.split(head).skip(1) {
            let rest = rest.trim_start().trim_start_matches('(');
            if let Some(end) = rest.find(|c: char| c.is_whitespace() || c == ')') {
                // A macro's template (`(defstruct ~name ..)`) is no type.
                if rest.starts_with(char::is_uppercase) {
                    names.push(rest[..end].to_string());
                }
            }
        }
    }
    names
}

/// The symbols of `forms` that start with a capital: the type names an
/// argument may hand to reflection.
fn capitalised(forms: &[Form], into: &mut Vec<String>) {
    for f in forms {
        match &f.kind {
            FormKind::Sym(n) if n.starts_with(char::is_uppercase) => into.push(n.clone()),
            FormKind::List(items) | FormKind::Vec(items) | FormKind::Map(items) => {
                capitalised(items, into)
            }
            _ => {}
        }
    }
}

/// The text of the expander's answer to every reflection call on every
/// name: `(OP NAME FORM)`, or `(OP NAME :error "MESSAGE")` for the error
/// the call ends the expansion with.
fn answers_text(ctx: &ExpandCtx, names: &[String], pos: &Pos) -> String {
    let mut out = Vec::new();
    for name in names {
        let sym = Form::new(FormKind::Sym(name.clone()), pos.clone());
        for op in REFLECTION_CALLS {
            out.push(match ctx.reflect(op, &sym, pos) {
                Ok(answer) => format!("({op} {name} {answer})"),
                Err(e) => {
                    let message = Form::new(FormKind::Str(e.kind.to_string()), pos.clone());
                    format!("({op} {name} :error {message})")
                }
            });
        }
    }
    out.join(" ")
}

/// `JitRunner` with a record of every run of the macros named `wanted`
/// (every macro when `None`): the call, the counter on either side (each
/// probe takes one number, so the counter after is the second probe's,
/// less one), the structs and the result.
struct Recording {
    inner: JitRunner,
    wanted: Option<&'static str>,
    type_names: Vec<String>,
    runs: Vec<Run>,
    modules: HashMap<String, usize>,
}

impl MacroRunner for Recording {
    fn run(&mut self, m: &MacroDef, args: Vec<Form>, ctx: &ExpandCtx) -> Result<Form, ExpandError> {
        if self.wanted.is_some_and(|w| w != m.name) {
            return self.inner.run(m, args, ctx);
        }
        let pos = ctx.call_pos().clone();
        let start = counter(&ctx.gensym("probe", &pos));
        let call = format!(
            "({}{})",
            m.name,
            args.iter().map(|a| format!(" {a}")).collect::<String>()
        );
        let mut names = self.type_names.clone();
        capitalised(&args, &mut names);
        let answers = answers_text(ctx, &names, &pos);
        let before = self.inner.texts.len();
        let result = self.inner.run(m, args, ctx);
        if self.inner.texts.len() > before {
            self.modules.insert(m.key.clone(), before);
        }
        let after = counter(&ctx.gensym("probe", &pos)) - 1;
        self.runs.push(Run {
            def: m.clone(),
            k: self.modules.get(&m.key).copied(),
            call,
            start,
            after,
            answers,
            result: result.clone(),
        });
        result
    }
}

/// What expanding `source` through the JIT runner made of the calls of
/// the macro `wanted` (all macros when `None`), and the lIR text of every
/// macro module. An expansion that fails ends there, its run recorded.
pub fn expand(source: &str, wanted: Option<&'static str>) -> (Vec<Run>, Vec<(String, String)>) {
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx).expect("the prelude expands");
    let forms = read_all(source, "scenario.fib").expect("the scenario reads");
    let mut recording = Recording {
        inner: JitRunner::new(prelude).expect("a JIT runner"),
        wanted,
        type_names: type_names(source),
        runs: Vec::new(),
        modules: HashMap::new(),
    };
    let _ = expand_program(forms, &mut ctx, &mut recording);
    (recording.runs, recording.inner.texts)
}

fn scratch(name: &str) -> PathBuf {
    shared_dir("macros").join(name)
}

/// What `jit-demo macro` must print for a run: the expansion and the
/// counter, or the error that ended it. An error of the module's own run
/// (`MacroFailed`) is printed as its message: the demo has no macro name
/// to put in front of it.
pub fn expected_output(run: &Run) -> String {
    match &run.result {
        Ok(form) => format!("expansion: {form}\ncounter: {}\n", run.after),
        Err(e) => match &e.kind {
            ExpandErrorKind::MacroFailed { message, .. } => {
                format!("-- expansion failed\n{message}\n")
            }
            kind => format!("-- expansion failed\n{kind}\n"),
        },
    }
}

/// `token` with its number raised by `by` when it is a gensym, `#prefix.N`.
fn shift_token(token: &str, by: u64) -> String {
    let shifted = token
        .strip_prefix('#')
        .and_then(|rest| rest.rsplit_once('.'))
        .and_then(|(prefix, n)| Some(format!("#{prefix}.{}", n.parse::<u64>().ok()? + by)));
    shifted.unwrap_or_else(|| token.to_string())
}

/// `line` with every gensym in it, `#prefix.N`, raised by `by`.
fn shift_line(line: &str, by: u64) -> String {
    let delimiter = |c: char| c.is_whitespace() || "()[]{}\"".contains(c);
    let (mut out, mut token) = (String::new(), String::new());
    for c in line.chars() {
        if delimiter(c) {
            out += &shift_token(&token, by);
            token.clear();
            out.push(c);
        } else {
            token.push(c);
        }
    }
    out + &shift_token(&token, by)
}

/// What `expected_output` of a successful run becomes when the call is
/// run `times` times in a row on one module: each run makes as many
/// gensyms as the first (`after - start`) and the numbers go on from
/// where the one before stopped, the counter line included.
pub fn expected_repeated(run: &Run, times: u64) -> String {
    let per_call = run.after - run.start;
    let once = expected_output(run);
    (0..times)
        .flat_map(|i| {
            once.lines()
                .map(move |line| match line.strip_prefix("counter: ") {
                    Some(n) => format!(
                        "counter: {}\n",
                        n.parse::<u64>().expect("a number") + i * per_call
                    ),
                    None => shift_line(line, i * per_call) + "\n",
                })
        })
        .collect()
}

/// What `jit-demo macro` makes of `run` with the module text `text`, run
/// `times` times in a row: the whole run, whatever its status, and the
/// arguments it was given.
pub fn demo_run(label: &str, run: &Run, text: &str, times: u64) -> (Vec<String>, Output) {
    let k = run.k.expect("the macro has a module");
    // The thread's id keeps two tests that run the same macro at once from
    // writing, and then removing, one file.
    let thread = format!("{:?}", std::thread::current().id()).replace(['(', ')'], "");
    let module = scratch(&format!("{label}-{k}-{}-{thread}.lir", run.start));
    std::fs::write(&module, text).expect("the module text is written");
    let rest = if run.def.rest.is_some() {
        "rest"
    } else {
        "fixed"
    };
    let args: Vec<String> = [
        "macro",
        module.to_str().expect("utf-8"),
        &k.to_string(),
        &run.start.to_string(),
        &run.def.params.len().to_string(),
        rest,
        &run.call,
        &run.answers,
        &times.to_string(),
    ]
    .iter()
    .map(|a| a.to_string())
    .collect();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_demo(&refs);
    let _ = std::fs::remove_file(&module);
    (args, out)
}

/// What `jit-demo macro` prints for `run`, whose module text is `text`,
/// when it runs the call `times` times in a row.
pub fn demo_output(label: &str, run: &Run, text: &str, times: u64) -> String {
    let (args, out) = demo_run(label, run, text, times);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let got = clean_stdout(&refs, out);
    println!("{label}: {}\n{got}", run.call);
    got
}

/// A program: some definitions and a macro whose calls the scenario runs.
pub struct Scenario {
    pub name: &'static str,
    pub source: &'static str,
    /// The macro whose calls are compared.
    pub wanted: &'static str,
    /// How many calls of the macro the program makes.
    pub calls: usize,
}

/// Runs every call of the scenario's macro from fibber and compares it
/// with the Rust runner's.
pub fn compare(s: &Scenario) {
    let (runs, texts) = expand(s.source, Some(s.wanted));
    assert_eq!(runs.len(), s.calls, "{}: the calls of {}", s.name, s.wanted);
    for run in &runs {
        let k = run.k.expect("the macro has a module");
        let got = demo_output(s.name, run, &texts[k].1, 1);
        assert_eq!(got, expected_output(run), "{}: {}", s.name, run.call);
    }
}
