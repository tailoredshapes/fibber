use lir_core::borrow::BorrowChecker;
use lir_core::parser::{ParseResult, Parser};
use lir_core::types::TypeChecker;
use std::io::Read;
fn main() {
    let mode = std::env::args().nth(1).unwrap();
    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src).unwrap();
    match mode.as_str() {
        "borrow" | "types" => {
            let items = match Parser::new(&src).parse_items() { Ok(i) => i, Err(e) => { println!("PARSE ERR {:?}", e); return; } };
            for it in items { if let ParseResult::Function(f) = it {
                if mode == "borrow" {
                    match BorrowChecker::new().check_function(&f) {
                        Ok(()) => println!("{}: borrow OK", f.name),
                        Err(es) => println!("{}: borrow ERR {:?}", f.name, es.iter().map(|e| e.to_string()).collect::<Vec<_>>()),
                    }
                } else {
                    match TypeChecker::new().check_function(&f) {
                        Ok(()) => println!("{}: types OK", f.name),
                        Err(e) => println!("{}: types ERR {}", f.name, e),
                    }
                }
            }}
        }
        "parse" => {
            // each line is a separate input
            for line in src.lines() {
                let l = line.to_string();
                let r = std::panic::catch_unwind(|| Parser::new(&l).parse_items().map(|v| v.len()));
                match r { Ok(Ok(n)) => println!("OK({n})  {l}"), Ok(Err(e)) => println!("ERR     {l}  -> {:?}", e), Err(_) => println!("PANIC   {l}") }
            }
        }
        _ => {}
    }
}
