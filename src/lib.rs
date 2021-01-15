#![deny(rust_2018_idioms)]

mod regexp;

mod ast;
mod disjoint;
mod ranges;

pub mod class;
pub mod parser;

pub use automata;
pub use regexp::*;
