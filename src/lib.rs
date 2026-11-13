//! Módulos principales del intérprete RLox.

pub mod compiler;
pub mod frontend;
pub mod interpreter;

// exports compatibles.
pub use compiler::{chunk, class, closure, function, value, vm, vm_class};
pub use frontend::{expr, parser, scanner, stmt, token};
pub use interpreter::{checker, environment, resolver};
