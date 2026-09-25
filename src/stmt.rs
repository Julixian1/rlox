use crate::expr::Expr;
use crate::token::Token;

pub trait StmtVisitor<R> {
    fn visit_expression_stmt(&mut self, expression: &Expr) -> R;
    fn visit_print_stmt(&mut self, expression: &Expr) -> R;
    fn visit_var_stmt(&mut self, name: &Token, initializer: &Option<Expr>) -> R;
    fn visit_block_stmt(&mut self, statements: &[Stmt]) -> R;
    fn visit_if_stmt(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: &Option<Box<Stmt>>,
    ) -> R;
    fn visit_while_stmt(&mut self, condition: &Expr, body: &Stmt) -> R;
    fn visit_function_stmt(&mut self, name: &Token, params: &[Token], body: &[Stmt]) -> R;
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
        body: Vec<Stmt>,
    },
    Return {
        keyword: Token,
        value: Option<Expr>,
    },
}

impl Stmt {
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
