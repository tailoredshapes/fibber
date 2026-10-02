// The Rust side of compiler/tests/emit/resume.sh: the hand-built builders of unit-resume.fib run through
// crates/fibc/src/resume.rs (copied, with its hash iterations made ordered; see resume.sh) and printed as
// `== NAME`, `frame: ...` and the function text, or `error: MSG`. unit-resume.fib builds the same builders
// with emit.ir and prints the same words; the two outputs must be byte for byte equal.
mod compile {
    #[derive(Debug)]
    pub struct Unsupported(pub String);
}
mod objects {
    use crate::value::LirTy;
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum Frame {
        Val(LirTy),
        Bytes(u64),
    }
}
mod ir;
mod resume;
mod value;

use ir::{FnBuilder, LirTy, V};
use objects::Frame;
use resume::{make_resumable, Shape};

const CAP: &str = "(load i64 (getelementptr %struct.task.t env (i32 0) (i32 10)))";
const STORE_POINT: &str = "(store (i32 1) (getelementptr %struct.task.t env (i32 0) (i32 11)))";
const PARK: &str = "(call @fib.await-or-park env (ptr null))";

fn env() -> Vec<(LirTy, String)> {
    vec![(LirTy::Ptr, "env".to_string())]
}

fn run(name: &str, mut f: FnBuilder, prologue: usize, points: &[&str]) {
    let pts: Vec<String> = points.iter().map(|s| s.to_string()).collect();
    let shape = Shape {
        tsname: "task.t",
        prologue,
        points: &pts,
        complete: "fib.complete.t",
        point_field: 11,
    };
    println!("== {name}");
    match make_resumable(&mut f, &shape) {
        Ok(frame) => {
            let words: Vec<String> = frame
                .iter()
                .map(|x| match x {
                    Frame::Val(t) => format!("val:{}", t.text()),
                    Frame::Bytes(n) => format!("bytes:{n}"),
                })
                .collect();
            println!("frame: {}", words.join(" "));
            print!("{}", f.render());
        }
        Err(e) => println!("error: {}", e.0),
    }
}

fn val(f: &mut FnBuilder, text: &str) -> V {
    f.val(text, LirTy::I64)
}

fn park_1(name: &str, label: &str, ret_operand: &str) {
    let mut f = FnBuilder::new("l.f.1", Some(LirTy::I64), env(), false);
    let cap = f.val(CAP, LirTy::I64);
    let x = val(&mut f, &format!("(add {} (i64 1))", cap.text()));
    f.stmt(STORE_POINT);
    let parked = f.val(PARK, LirTy::I1);
    f.term(&format!("(br {} park2 resume3)", parked.text()));
    f.open("park2");
    f.term("(ret)");
    f.open("resume3");
    let r = if ret_operand.is_empty() { x.text().to_string() } else { ret_operand.to_string() };
    f.term(&format!("(ret {r})"));
    run(name, f, 1, &[label]);
}

fn allocas() {
    let mut f = FnBuilder::new("l.f.2", None, env(), false);
    let a = f.entry_alloca("i64", 24);
    let b = f.entry_alloca("i64", 0);
    let cap = f.val(CAP, LirTy::I64);
    let x = val(&mut f, &format!("(add {} (i64 1))", cap.text()));
    f.stmt(&format!("(store {} {a})", x.text()));
    f.stmt(STORE_POINT);
    let parked = f.val(PARK, LirTy::I1);
    f.term(&format!("(br {} park6 resume7)", parked.text()));
    f.open("park6");
    f.term("(ret)");
    f.open("resume7");
    f.stmt(&format!("(store {} {b})", x.text()));
    f.term("(ret)");
    run("allocas", f, 1, &["resume7"]);
}

fn join() {
    let mut f = FnBuilder::new("l.f.3", Some(LirTy::I64), env(), false);
    let cap = f.val(CAP, LirTy::I64);
    let x = val(&mut f, &format!("(add {} (i64 1))", cap.text()));
    let c = f.val(&format!("(icmp sgt {} (i64 0))", x.text()), LirTy::I1);
    f.term(&format!("(br {} a b)", c.text()));
    f.open("a");
    f.term("(br join)");
    f.open("b");
    f.term("(br join)");
    f.open("join");
    let p = f.phi(
        LirTy::I64,
        &[("a".to_string(), x.text().to_string()), ("b".to_string(), "(i64 7)".to_string())],
    );
    f.stmt(STORE_POINT);
    let parked = f.val(PARK, LirTy::I1);
    f.term(&format!("(br {} park resume)", parked.text()));
    f.open("park");
    f.term("(ret)");
    f.open("resume");
    let s = val(&mut f, &format!("(add {} {})", p.text(), x.text()));
    f.term(&format!("(ret {})", s.text()));
    f.open("unreached");
    f.stmt("(call @fib.trap-c)");
    f.term("(unreachable)");
    run("join", f, 1, &["resume"]);
}

fn order() {
    let mut f = FnBuilder::new("l.f.4", Some(LirTy::I64), env(), false);
    let cap = f.val(CAP, LirTy::I64);
    let mut last = cap.text().to_string();
    let mut made = Vec::new();
    for i in 1..=10 {
        let v = val(&mut f, &format!("(add {last} (i64 {i}))"));
        last = v.text().to_string();
        made.push(last.clone());
    }
    f.stmt(STORE_POINT);
    let parked = f.val(PARK, LirTy::I1);
    f.term(&format!("(br {} park resume)", parked.text()));
    f.open("park");
    f.term("(ret)");
    f.open("resume");
    let r = val(&mut f, &format!("(add {} {})", made[7], made[8]));
    let r2 = val(&mut f, &format!("(add {} {})", r.text(), made[9]));
    f.term(&format!("(ret {})", r2.text()));
    run("order", f, 1, &["resume"]);
}

fn looping() {
    let mut f = FnBuilder::new("l.f.5", Some(LirTy::I64), env(), false);
    let cap = f.val(CAP, LirTy::I64);
    let t2 = val(&mut f, &format!("(add {} (i64 1))", cap.text()));
    f.term("(br loop)");
    f.open("loop");
    let t3 = f.phi(LirTy::I64, &[("entry".to_string(), t2.text().to_string())]);
    let t4 = val(&mut f, &format!("(add {} (i64 1))", t3.text()));
    f.stmt(STORE_POINT);
    let parked = f.val(PARK, LirTy::I1);
    f.term(&format!("(br {} park resume)", parked.text()));
    f.open("park");
    f.term("(ret)");
    f.open("resume");
    f.patch_phi("loop", t3.text(), "resume", t4.text());
    let t6 = f.val(&format!("(icmp slt {} (i64 10))", t4.text()), LirTy::I1);
    f.term(&format!("(br {} loop done)", t6.text()));
    f.open("done");
    f.term(&format!("(ret {})", t4.text()));
    run("loop", f, 1, &["resume"]);
}

// Two phis of one join take demoted values from the same block: their loads go before its terminator.
fn edges() {
    let mut f = FnBuilder::new("l.f.6", Some(LirTy::I64), env(), false);
    let cap = f.val(CAP, LirTy::I64);
    let mut last = cap.text().to_string();
    let mut made = Vec::new();
    for i in 1..=9 {
        let v = val(&mut f, &format!("(add {last} (i64 {i}))"));
        last = v.text().to_string();
        made.push(last.clone());
    }
    let c = f.val(&format!("(icmp sgt {last} (i64 0))"), LirTy::I1);
    f.term(&format!("(br {} a b)", c.text()));
    f.open("a");
    f.term("(br join)");
    f.open("b");
    f.term("(br join)");
    f.open("join");
    let p1 = f.phi(LirTy::I64, &[("a".to_string(), made[7].clone()), ("b".to_string(), "(i64 1)".to_string())]);
    let p2 = f.phi(LirTy::I64, &[("a".to_string(), made[8].clone()), ("b".to_string(), "(i64 2)".to_string())]);
    f.stmt(STORE_POINT);
    let parked = f.val(PARK, LirTy::I1);
    f.term(&format!("(br {} park resume)", parked.text()));
    f.open("park");
    f.term("(ret)");
    f.open("resume");
    let s1 = val(&mut f, &format!("(add {} {})", p1.text(), p2.text()));
    let s2 = val(&mut f, &format!("(add {} {})", made[7], made[8]));
    let s3 = val(&mut f, &format!("(add {} {})", s1.text(), s2.text()));
    f.term(&format!("(ret {})", s3.text()));
    run("edges", f, 1, &["resume"]);
}

fn main() {
    park_1("park1", "resume3", "");
    allocas();
    join();
    order();
    looping();
    park_1("no-continuation", "nope", "");
    park_1("unknown-type", "resume3", "t99");
    edges();
}
