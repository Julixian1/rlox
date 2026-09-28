//! Módulos principales del intérprete RLox.

/// Gestión de entornos y ámbitos de variables.
pub mod environment;
/// Representación de nodos de expresiones en el AST.
pub mod expr;
/// Representación e invocación de funciones de Lox.
pub mod function;
/// Ejecución y evaluación del AST.
pub mod interpreter;
/// Analizador sintáctico que construye el AST desde tokens.
pub mod parser;
/// Analizador semántico y resolución de alcance de variables.
pub mod resolver;
/// Analizador léxico que convierte código fuente en tokens.
pub mod scanner;
/// Representación de nodos de sentencias en el AST.
pub mod stmt;
/// Definición de tipos de tokens y estructura Token.
pub mod token;
/// Definición de los valores en tiempo de ejecución de Lox.
pub mod value;
