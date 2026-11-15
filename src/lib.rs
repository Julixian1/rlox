//! Biblioteca principal del intérprete y compilador de Lox (`rlox`).
//!
//! Expone el frontend (escáner, parser, AST), el backend tree-walk (checker,
//! resolver, intérprete) y los backends basados en bytecode (Stack VM y Register VM).

pub mod compiler;
pub mod frontend;
pub mod interpreter;

pub use compiler::{chunk, class, closure, function, opt, value, vm, vm_class};
pub use frontend::{expr, parser, scanner, stmt, token};
pub use interpreter::{checker, environment, resolver};
