//! What the result of every expansion step must pass before the walk
//! goes on (§3.16): its forms count against [`Limits::max_forms`], and
//! its literals are checked as the reader checks the literals it reads
//! (§1.1), since a macro can build `(Int 300 :i8)`, which no source text
//! can spell.
//!
//! [`Limits::max_forms`]: super::Limits

use crate::syntax::{check_literal, Form, FormKind, Pos};

use super::ctx::ExpandCtx;
use super::error::{ExpandError, ExpandErrorKind as K};

impl ExpandCtx {
    /// Admits `form`, the result of an expansion of the call at `pos`:
    /// counts each of its forms, failing with [`K::TooLarge`] past the
    /// limit, and checks (and, for an `f32` float, rounds) each literal,
    /// failing with [`K::BadLiteral`] at the literal. The walk uses an
    /// explicit stack: a result is not yet known to be within the depth
    /// limit.
    pub(crate) fn admit(&mut self, form: &mut Form, pos: &Pos) -> Result<(), ExpandError> {
        let mut stack = vec![form];
        while let Some(f) = stack.pop() {
            self.forms += 1;
            if self.forms > self.limits.max_forms {
                let limit = self.limits.max_forms;
                return Err(ExpandError::new(K::TooLarge { limit }, pos));
            }
            check_literal(&mut f.kind)
                .map_err(|error| ExpandError::new(K::BadLiteral { error }, &f.pos))?;
            if let FormKind::List(items) | FormKind::Vec(items) | FormKind::Map(items) = &mut f.kind
            {
                stack.extend(items.iter_mut());
            }
        }
        Ok(())
    }
}
