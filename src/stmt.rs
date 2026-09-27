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
}

// Nodos de statement del AST.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expression {
        expression: Expr,
    },
    Print {
        expression: Expr,
    },
    Var {
        name: Token,
        initializer: Option<Expr>,
    },
    Block {
        statements: Vec<Stmt>,
    },
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While {
        condition: Expr,
        body: Box<Stmt>,
    },
    Function {
        name: Token,
        params: Vec<Token>,
        body: Rc<Vec<Stmt>>,
    },
    Return {
        keyword: Token,
        value: Option<Expr>,
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
        }
    }
}
