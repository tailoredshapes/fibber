//! `fibgen`: random well-typed fibber programs, run through the reference
//! interpreter (`fibref`) and checked against its memory audit and an
//! independent model of their result (spec/method.md, rule 5).
//!
//! [`gen`] builds a program from a seed; [`print`] writes it as fibber
//! source; [`model`] computes the value `main` must return; [`run`] runs
//! it through the interpreter with a time limit and classifies the
//! outcome; [`shrink`] minimises a finding; [`report`] writes it as a
//! case file; [`coverage`] counts the constructs a run exercised.

#![forbid(unsafe_code)]

pub mod ast;
pub mod coverage;
pub mod driver;
pub mod gen;
pub mod macros;
pub mod model;
pub mod pipe;
pub mod print;
pub mod report;
pub mod rng;
pub mod run;
pub mod shrink;
pub mod ty;
