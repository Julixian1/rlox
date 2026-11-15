//! Representación de sentencias (statements) del AST y trait visitor correspondiente.

use crate::expr::Expr;
use crate::token::Token;

use std::rc::Rc;

/// Trait patrón Visitor para recorrer y procesar sentencias (statements) del AST.
pub trait StmtVisitor<R> {
    /// Procesa una sentencia de expresión (`expr;`).
    fn visit_expression_stmt(&mut self, expression: &Expr) -> R;
    /// Procesa una sentencia de impresión (`print expr;`).
    fn visit_print_stmt(&mut self, expression: &Expr) -> R;
    /// Procesa una declaración de variable (`var name = init;`).
    fn visit_var_stmt(&mut self, name: &Token, initializer: &Option<Expr>) -> R;
    /// Procesa un bloque de código entre llaves `{ ... }`.
    fn visit_block_stmt(&mut self, statements: &[Stmt]) -> R;
    /// Procesa una sentencia condicional `if (cond) then_branch else else_branch`.
    fn visit_if_stmt(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: &Option<Box<Stmt>>,
    ) -> R;
    /// Procesa un bucle `while (cond) body`.
    fn visit_while_stmt(&mut self, condition: &Expr, body: &Stmt) -> R;
    /// Procesa una declaración de función (`fun name(params) { body }`).
    fn visit_function_stmt(&mut self, name: &Token, params: &[Token], body: &Rc<Vec<Stmt>>) -> R;
    /// Procesa una sentencia de retorno (`return expr;`).
    fn visit_return_stmt(&mut self, keyword: &Token, value: &Option<Expr>) -> R;
    /// Procesa una sentencia `break;`.
    fn visit_break_stmt(&mut self, keyword: &Token) -> R;
    /// Procesa una sentencia `continue;`.
    fn visit_continue_stmt(&mut self, keyword: &Token) -> R;
    /// Procesa un bucle `for (init; cond; inc) body`.
    fn visit_for_stmt(
        &mut self,
        initializer: &Option<Box<Stmt>>,
        condition: &Option<Expr>,
        increment: &Option<Expr>,
        body: &Stmt,
    ) -> R;
    /// Procesa una declaración de clase (`class name < superclass { methods }`).
    fn visit_class_stmt(&mut self, name: &Token, superclass: &Option<Expr>, methods: &[Stmt]) -> R;
}

/// Nodos del Árbol de Sintaxis Abstracta (AST) que representan sentencias ejecutables.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// Sentencia de expresión evaluable (`expr;`).
    Expression {
        expression: Expr,
    },
    /// Sentencia de impresión en salida estándar (`print expr;`).
    Print {
        expression: Expr,
    },
    /// Declaración de variable (`var name = initializer;`).
    Var {
        name: Token,
        initializer: Option<Expr>,
    },
    /// Bloque de sentencias delimitado por llaves `{ ... }`.
    Block {
        statements: Vec<Stmt>,
    },
    /// Sentencia condicional bifurcada `if (cond) then_branch else else_branch`.
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    /// Bucle condicional `while (cond) body`.
    While {
        condition: Expr,
        body: Box<Stmt>,
    },
    /// Declaración de función o método (`fun name(params) { body }`).
    Function {
        name: Token,
        params: Vec<Token>,
        body: Rc<Vec<Stmt>>,
    },
    /// Sentencia de retorno explícito de función (`return expr;`).
    Return {
        keyword: Token,
        value: Option<Expr>,
    },
    /// Sentencia de terminación inmediata de bucle (`break;`).
    Break {
        keyword: Token,
    },
    /// Sentencia de continuación a la siguiente iteración de bucle (`continue;`).
    Continue {
        keyword: Token,
    },
    /// Bucle determinado con cláusulas `for (init; cond; inc) body`.
    For {
        initializer: Option<Box<Stmt>>,
        condition: Option<Expr>,
        increment: Option<Expr>,
        body: Box<Stmt>,
    },
    /// Declaración de clase con superclase opcional y lista de métodos.
    Class {
        name: Token,
        superclass: Option<Expr>,
        methods: Vec<Stmt>,
    },
}

impl Stmt {
    /// Acepta un visitante implementando el patrón Visitor para despachar el método `visit_*` correspondiente según el tipo de sentencia.
    pub fn accept<R>(&self, visitor: &mut impl StmtVisitor<R>) -> R {
        match self {
            Stmt::Expression { expression } => visitor.visit_expression_stmt(expression),
            Stmt::Print { expression } => visitor.visit_print_stmt(expression),
            Stmt::Var { name, initializer } => visitor.visit_var_stmt(name, initializer),
            Stmt::Block { statements } => visitor.visit_block_stmt(statements),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => visitor.visit_if_stmt(condition, then_branch, else_branch),
            Stmt::While { condition, body } => visitor.visit_while_stmt(condition, body),
            Stmt::Function { name, params, body } => {
                visitor.visit_function_stmt(name, params, body)
            }
            Stmt::Return { keyword, value } => visitor.visit_return_stmt(keyword, value),
            Stmt::Break { keyword } => visitor.visit_break_stmt(keyword),
            Stmt::Continue { keyword } => visitor.visit_continue_stmt(keyword),
            Stmt::For {
                initializer,
                condition,
                increment,
                body,
            } => visitor.visit_for_stmt(initializer, condition, increment, body),
            Stmt::Class {
                name,
                superclass,
                methods,
            } => visitor.visit_class_stmt(name, superclass, methods),
        }
    }
}
