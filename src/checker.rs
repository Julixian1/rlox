use std::collections::HashMap;
use std::rc::Rc;
 
use crate::expr::Expr;
use crate::stmt::{Stmt, StmtVisitor};
use crate::token::Token;
 
type Finding = (usize, String);
 
pub fn check(statements: &[Stmt]) -> Vec<String> {
    let mut findings = ReturnCheck::run(statements);
    findings.extend(DeclarationCheck::run(statements));
    findings.sort_by_key(|(line, _)| *line);
    findings.into_iter().map(|(_, message)| message).collect()
}
 
struct ReturnCheck {
    in_function: bool,
    findings: Vec<Finding>,
}
 
impl ReturnCheck {
    fn run(statements: &[Stmt]) -> Vec<Finding> {
        let mut pass = ReturnCheck {
            in_function: false,
            findings: Vec::new(),
        };
        for stmt in statements {
            stmt.accept(&mut pass);
        }
        pass.findings
    }
}
 
impl StmtVisitor<()> for ReturnCheck {
    // Estos statements no contienen otros statements: nada que recorrer.
    fn visit_expression_stmt(&mut self, _expression: &Expr) {}
    fn visit_print_stmt(&mut self, _expression: &Expr) {}
    fn visit_var_stmt(&mut self, _name: &Token, _initializer: &Option<Expr>) {}
 
    fn visit_block_stmt(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            stmt.accept(self);
        }
    }
 
    fn visit_if_stmt(
        &mut self,
        _condition: &Expr,
        then_branch: &Stmt,
        else_branch: &Option<Box<Stmt>>,
    ) {
        then_branch.accept(self);
        if let Some(else_stmt) = else_branch {
            else_stmt.accept(self);
        }
    }
 
    fn visit_while_stmt(&mut self, _condition: &Expr, body: &Stmt) {
        body.accept(self);
    }
 
    fn visit_function_stmt(&mut self, _name: &Token, _params: &[Token], body: &Rc<Vec<Stmt>>) {
        let enclosing = self.in_function;
        self.in_function = true;
        for stmt in body.iter() {
            stmt.accept(self);
        }
        self.in_function = enclosing;
    }
 
    fn visit_return_stmt(&mut self, keyword: &Token, _value: &Option<Expr>) {
        if !self.in_function {
            self.findings.push((
                keyword.line,
                format!(
                    "[line {}] Error at 'return': Can't return from top-level code.",
                    keyword.line
                ),
            ));
        }
    }
}
 

struct DeclarationCheck {
    scopes: Vec<HashMap<String, bool>>,
    findings: Vec<Finding>,
}
 
impl DeclarationCheck {
    fn run(statements: &[Stmt]) -> Vec<Finding> {
        let mut pass = DeclarationCheck {
            scopes: Vec::new(),
            findings: Vec::new(),
        };
        for stmt in statements {
            stmt.accept(&mut pass);
        }
        pass.findings
    }
 
    fn begin_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }
 
    fn end_scope(&mut self) {
        self.scopes.pop();
    }
 
    /// Declara la variable en el scope actual, todavía sin inicializar.
    fn declare(&mut self, name: &Token) {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(&name.lexeme) {
                self.findings.push((
                    name.line,
                    format!(
                        "[line {}] Error at '{}': Already a variable with this name in this scope.",
                        name.line, name.lexeme
                    ),
                ));
            }
            scope.insert(name.lexeme.clone(), false);
        }
    }
 
    fn define(&mut self, name: &Token) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.lexeme.clone(), true);
        }
    }
 
    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Variable { name } => {
                let uninitialized = self
                    .scopes
                    .last()
                    .is_some_and(|scope| scope.get(&name.lexeme) == Some(&false));
                if uninitialized {
                    self.findings.push((
                        name.line,
                        format!(
                            "[line {}] Error at '{}': Can't read local variable in its own initializer.",
                            name.line, name.lexeme
                        ),
                    ));
                }
            }
            Expr::Assign { value, .. } => self.check_expr(value),
            Expr::Binary { left, right, .. } | Expr::Logical { left, right, .. } => {
                self.check_expr(left);
                self.check_expr(right);
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                self.check_expr(callee);
                for arg in arguments {
                    self.check_expr(arg);
                }
            }
            Expr::Grouping { expression } => self.check_expr(expression),
            Expr::Unary { right, .. } => self.check_expr(right),
            Expr::Literal { .. } => {}
        }
    }
}
 
impl StmtVisitor<()> for DeclarationCheck {
    fn visit_expression_stmt(&mut self, expression: &Expr) {
        self.check_expr(expression);
    }
 
    fn visit_print_stmt(&mut self, expression: &Expr) {
        self.check_expr(expression);
    }
 
    fn visit_var_stmt(&mut self, name: &Token, initializer: &Option<Expr>) {
        self.declare(name);
        if let Some(init) = initializer {
            self.check_expr(init);
        }
        self.define(name);
    }
 
    fn visit_block_stmt(&mut self, statements: &[Stmt]) {
        self.begin_scope();
        for stmt in statements {
            stmt.accept(self);
        }
        self.end_scope();
    }
 
    fn visit_if_stmt(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: &Option<Box<Stmt>>,
    ) {
        self.check_expr(condition);
        then_branch.accept(self);
        if let Some(else_stmt) = else_branch {
            else_stmt.accept(self);
        }
    }
 
    fn visit_while_stmt(&mut self, condition: &Expr, body: &Stmt) {
        self.check_expr(condition);
        body.accept(self);
    }
 
    fn visit_function_stmt(&mut self, name: &Token, params: &[Token], body: &Rc<Vec<Stmt>>) {
        // El nombre queda definido antes del cuerpo: la función puede llamarse a sí misma.
        self.declare(name);
        self.define(name);
 
        self.begin_scope();
        for param in params {
            self.declare(param);
            self.define(param);
        }
        for stmt in body.iter() {
            stmt.accept(self);
        }
        self.end_scope();
    }
 
    fn visit_return_stmt(&mut self, _keyword: &Token, value: &Option<Expr>) {
        if let Some(expr) = value {
            self.check_expr(expr);
        }
    }
}
 