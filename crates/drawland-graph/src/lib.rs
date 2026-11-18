#![doc = include_str!("../README.md")]

pub mod api;
mod arg;
mod graph;
mod library;

pub use api::{Area, Features, Lines};
pub use arg::{describe, number, show_number, Arg, Lazy, Obj};
pub use drawland_core::grammar::{At, Grammar};
pub use graph::{find_at, Error, Graph, Result, VIEW_DEPTH};
pub use library::{is_remap, FUNCTIONS, PROPERTIES};
