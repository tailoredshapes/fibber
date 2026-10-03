//! Editor support for fibber (spec/bootstrap.md §9): `fibref complete`,
//! `fibref diagnostics` and the language server `fibref lsp`, all over
//! the reference front end.

pub mod analysis;
pub mod catalog;
pub mod cli;
pub mod complete;
pub mod context;
pub mod diagnostics;
pub mod lsp;
pub mod scope;
pub mod text;
pub mod transport;
