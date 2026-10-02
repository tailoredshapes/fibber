//! Traps with a message the lowering knows: the block ends there. In a
//! macro module (`Program::macro_module`) a trap fails the expansion
//! through the module's hook, and carries the position of the
//! expression being lowered, as the interpreter's run error does
//! (macros/bridge.rs).

use super::Cx;

/// `text` as the body of an lIR string literal.
pub fn lir_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

impl Cx<'_, '_> {
    /// A trap with a C-string message: the block ends here.
    pub fn trap_c(&mut self, msg: &str) {
        match self.at {
            Some(e) if self.p.macro_module => self.b.stmt(&format!(
                "(call @fib.trap-at-c (string \"{}\") (string \"{msg}\"))",
                lir_text(&e.pos.to_string())
            )),
            _ => self
                .b
                .stmt(&format!("(call @fib.trap-c (string \"{msg}\"))")),
        }
        self.b.term("(unreachable)");
    }
}
