use std::collections::{HashMap, HashSet};
 
use crate::expr::{Expr, ExprVisitor, LiteralValue};
use crate::stmt::{Stmt, StmtVisitor};
use crate::token::Token;
 
pub struct Resolver {
    scopes: Vec<HashSet<String>>,
    pub locals: HashMap<*const Expr, usize>,
}
 
impl Resolver {
    pub fn new() -> Self {
        Resolver {
            scopes: Vec::new(),
            locals: HashMap::new(),
        }
    }
 
    fn begin_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }
 
    fn end_scope(&mut self) {
        self.scopes.pop();
    }
 
    fn declare(&mut self, name: &Token) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.lexeme.clone());
        }
    }
 
    fn resolve_local(&mut self, expr_ptr: *const Expr, name: &Token) {
        for (i, scope) in self.scopes.iter().rev().enumerate() {
            if scope.contains(&name.lexeme) {
                self.locals.insert(expr_ptr, i);
                return;
            }
        }
        // No encontrada en ningún scope local → es global, no se registra.
    }
 
    fn resolve_function(&mut self, params: &[Token], body: &[Stmt]) {
        self.begin_scope();
        for param in params {
            self.declare(param);
        }
        self.resolve_stmts(body);
        self.end_scope();
    }
 
    pub fn resolve_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.resolve_stmt(stmt);
        }
    }
 
    fn resolve_stmt(&mut self, stmt: &Stmt) {
        stmt.accept(self);
    }
 
    pub fn resolve_expr(&mut self, expr: &Expr) {
        let ptr = expr as *const Expr;
        match expr {
            Expr::Variable { name } => {
                self.resolve_local(ptr, name);
            }
            Expr::Assign { name, value } => {
                self.resolve_expr(value);
                self.resolve_local(ptr, name);
            }
            Expr::Binary { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                self.resolve_expr(callee);
                for arg in arguments {
                    self.resolve_expr(arg);
                }
            }
            Expr::Grouping { expression } => {
                self.resolve_expr(expression);
            }
            Expr::Literal { .. } => {}
            Expr::Logical { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            Expr::Unary { right, .. } => {
                self.resolve_expr(right);
            }
        }
    }
}
 
impl StmtVisitor<()> for Resolver {
    fn visit_block_stmt(&mut self, statements: &[Stmt]) {
        self.begin_scope();
        self.resolve_stmts(statements);
        self.end_scope();
    }
 
    fn visit_var_stmt(&mut self, name: &Token, initializer: &Option<Expr>) {
        self.declare(name);
        if let Some(init) = initializer {
            self.resolve_expr(init);
        }
    }
 
    fn visit_function_stmt(
        &mut self,
        name: &Token,
        params: &[Token],
        body: &std::rc::Rc<Vec<Stmt>>,
    ) {
        self.declare(name);
        self.resolve_function(params, body);
    }
 
    fn visit_expression_stmt(&mut self, expression: &Expr) {
        self.resolve_expr(expression);
    }
 
    fn visit_if_stmt(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: &Option<Box<Stmt>>,
    ) {
        self.resolve_expr(condition);
        self.resolve_stmt(then_branch);
        if let Some(else_stmt) = else_branch {
            self.resolve_stmt(else_stmt);
        }
    }
 
    fn visit_print_stmt(&mut self, expression: &Expr) {
        self.resolve_expr(expression);
    }
 
    fn visit_return_stmt(&mut self, _keyword: &Token, value: &Option<Expr>) {
        if let Some(expr) = value {
            self.resolve_expr(expr);
        }
    }
 
    fn visit_while_stmt(&mut self, condition: &Expr, body: &Stmt) {
        self.resolve_expr(condition);
        self.resolve_stmt(body);
    }
}
 
impl ExprVisitor<()> for Resolver {
    fn visit_literal_expr(&mut self, _value: &LiteralValue) {}
    fn visit_unary_expr(&mut self, _operator: &Token, right: &Expr) {
        self.resolve_expr(right);
    }
    fn visit_binary_expr(&mut self, left: &Expr, _operator: &Token, right: &Expr) {
        self.resolve_expr(left);
        self.resolve_expr(right);
    }
    fn visit_grouping_expr(&mut self, expression: &Expr) {
        self.resolve_expr(expression);
    }
    fn visit_variable_expr(&mut self, _name: &Token) {}
    fn visit_assign_expr(&mut self, _name: &Token, value: &Expr) {
        self.resolve_expr(value);
    }
    fn visit_logical_expr(&mut self, left: &Expr, _operator: &Token, right: &Expr) {
        self.resolve_expr(left);
        self.resolve_expr(right);
    }
    fn visit_call_expr(&mut self, callee: &Expr, _paren: &Token, arguments: &[Expr]) {
        self.resolve_expr(callee);
        for arg in arguments {
            self.resolve_expr(arg);
        }
    }
}
 