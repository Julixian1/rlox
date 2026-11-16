//! Backend alternativo: compilador y máquina virtual basada en registros (Register VM).
//!
//! Reutiliza el frontend, las estructuras del AST y la representación unificada de `Value`.

pub mod chunk;
pub mod closure;
pub mod codegen;
pub mod vm;
