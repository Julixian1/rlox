//! Backend alternativo: compilador + VM basada en registros.
//! Reutiliza el frontend y `Value` del backend de pila.
 
pub mod chunk;
pub mod codegen;
pub mod vm;
pub mod closure;