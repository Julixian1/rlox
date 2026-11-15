use std::collections::HashMap;
use std::rc::Rc;

use crate::expr::Expr;
use crate::stmt::{Stmt, StmtVisitor};
use crate::token::Token;

type Finding = (usize, String);

pub fn check(statements: &[Stmt]) -> Vec<String> {
    let mut findings = ReturnCheck::run(statements);
    findings.extend(DeclarationCheck::run(statements));
    findings.extend(LoopCheck::run(statements));
    findings.sort_by_key(|(line, _)| *line);
    findings.into_iter().map(|(_, message)| message).collect()
}

#[derive(Copy, Clone, PartialEq)]
enum FunctionContext {
    None,
    Function,
    Initializer,
}

struct ReturnCheck {
    context: FunctionContext,
    findings: Vec<Finding>,
}

impl ReturnCheck {
    fn run(statements: &[Stmt]) -> Vec<Finding> {
        let mut pass = ReturnCheck {
            context: FunctionContext::None,
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
        let enclosing = self.context;
        self.context = FunctionContext::Function;
        for stmt in body.iter() {
            stmt.accept(self);
        }
        self.context = enclosing;
    }

    fn visit_return_stmt(&mut self, keyword: &Token, value: &Option<Expr>) {
        if self.context == FunctionContext::None {
            self.findings.push((
                keyword.line,
                format!(
                    "[line {}] Error at 'return': Can't return from top-level code.",
                    keyword.line
                ),
            ));
        } else if self.context == FunctionContext::Initializer && value.is_some() {
            self.findings.push((
                keyword.line,
                format!(
                    "[line {}] Error at 'return': Can't return a value from an initializer.",
                    keyword.line
                ),
            ));
        }
    }

    fn visit_break_stmt(&mut self, _keyword: &Token) {}

    fn visit_continue_stmt(&mut self, _keyword: &Token) {}

    fn visit_for_stmt(
        &mut self,
        initializer: &Option<Box<Stmt>>,
        _condition: &Option<Expr>,
        _increment: &Option<Expr>,
        body: &Stmt,
    ) {
        if let Some(init) = initializer {
            init.accept(self);
        }
        body.accept(self);
    }

    fn visit_class_stmt(&mut self, _name: &Token, _superclass: &Option<Expr>, methods: &[Stmt]) {
        for method in methods {
            if let Stmt::Function { name, body, .. } = method {
                let enclosing = self.context;
                if name.lexeme == "init" {
                    self.context = FunctionContext::Initializer;
                } else {
                    self.context = FunctionContext::Function;
                }
                for stmt in body.iter() {
                    stmt.accept(self);
                }
                self.context = enclosing;
            }
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
enum ClassContext {
    None,
    Class,
    Subclass,
}

struct DeclarationCheck {
    scopes: Vec<HashMap<String, bool>>,
    findings: Vec<Finding>,
    in_class: bool,
    class_context: ClassContext,
}

impl DeclarationCheck {
    fn run(statements: &[Stmt]) -> Vec<Finding> {
        let mut pass = DeclarationCheck {
            scopes: Vec::new(),
            findings: Vec::new(),
            in_class: false,
            class_context: ClassContext::None,
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
            Expr::AssignModify { name, value, .. } => {
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
                self.check_expr(value);
            }
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
            Expr::Ternary {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expr(condition);
                self.check_expr(then_branch);
                self.check_expr(else_branch);
            }
            Expr::Literal { .. } => {}
            Expr::Get { object, .. } => self.check_expr(object),
            Expr::Set { object, value, .. } | Expr::SetModify { object, value, .. } => {
                self.check_expr(object);
                self.check_expr(value);
            }
            Expr::List { elements, .. } => {
                for element in elements {
                    self.check_expr(element);
                }
            }
            Expr::IndexGet { object, index, .. } => {
                self.check_expr(object);
                self.check_expr(index);
            }
            Expr::IndexSet {
                object,
                index,
                value,
                ..
            }
            | Expr::IndexSetModify {
                object,
                index,
                value,
                ..
            } => {
                self.check_expr(object);
                self.check_expr(index);
                self.check_expr(value);
            }
            Expr::This { keyword } => {
                if !self.in_class {
                    self.findings.push((
                        keyword.line,
                        format!(
                            "[line {}] Error at 'this': Can't use 'this' outside of a class.",
                            keyword.line
                        ),
                    ));
                }
            }
            Expr::Super { keyword, .. } => match self.class_context {
                ClassContext::None => {
                    self.findings.push((
                        keyword.line,
                        format!(
                            "[line {}] Error at 'super': Can't use 'super' outside of a class.",
                            keyword.line
                        ),
                    ));
                }
                ClassContext::Class => {
                    self.findings.push((
                        keyword.line,
                        format!(
                            "[line {}] Error at 'super': Can't use 'super' in a class with no superclass.",
                            keyword.line
                        ),
                    ));
                }
                ClassContext::Subclass => {}
            },
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

    fn visit_class_stmt(&mut self, name: &Token, superclass: &Option<Expr>, methods: &[Stmt]) {
        let enclosing_in_class = self.in_class;
        let enclosing_context = self.class_context;

        self.in_class = true;
        self.class_context = if superclass.is_some() {
            ClassContext::Subclass
        } else {
            ClassContext::Class
        };

        self.declare(name);
        self.define(name);
        if let Some(super_expr) = superclass {
            self.check_expr(super_expr);
        }
        for method in methods {
            if let Stmt::Function { params, body, .. } = method {
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
        }

        self.in_class = enclosing_in_class;
        self.class_context = enclosing_context;
    }

    fn visit_break_stmt(&mut self, _keyword: &Token) {}

    fn visit_continue_stmt(&mut self, _keyword: &Token) {}

    fn visit_for_stmt(
        &mut self,
        initializer: &Option<Box<Stmt>>,
        condition: &Option<Expr>,
        increment: &Option<Expr>,
        body: &Stmt,
    ) {
        self.begin_scope();
        if let Some(init) = initializer {
            init.accept(self);
        }
        if let Some(cond) = condition {
            self.check_expr(cond);
        }
        if let Some(inc) = increment {
            self.check_expr(inc);
        }
        body.accept(self);
        self.end_scope();
    }
}

struct LoopCheck {
    loop_depth: usize,
    findings: Vec<Finding>,
}

impl LoopCheck {
    fn run(statements: &[Stmt]) -> Vec<Finding> {
        let mut pass = LoopCheck {
            loop_depth: 0,
            findings: Vec::new(),
        };
        for stmt in statements {
            stmt.accept(&mut pass);
        }
        pass.findings
    }
}

impl StmtVisitor<()> for LoopCheck {
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
        self.loop_depth += 1;
        body.accept(self);
        self.loop_depth -= 1;
    }

    fn visit_for_stmt(
        &mut self,
        initializer: &Option<Box<Stmt>>,
        _condition: &Option<Expr>,
        _increment: &Option<Expr>,
        body: &Stmt,
    ) {
        if let Some(init) = initializer {
            init.accept(self);
        }
        self.loop_depth += 1;
        body.accept(self);
        self.loop_depth -= 1;
    }

    fn visit_function_stmt(&mut self, _name: &Token, _params: &[Token], body: &Rc<Vec<Stmt>>) {
        let enclosing = self.loop_depth;
        self.loop_depth = 0;
        for stmt in body.iter() {
            stmt.accept(self);
        }
        self.loop_depth = enclosing;
    }

    fn visit_return_stmt(&mut self, _keyword: &Token, _value: &Option<Expr>) {}

    fn visit_break_stmt(&mut self, keyword: &Token) {
        if self.loop_depth == 0 {
            self.findings.push((
                keyword.line,
                format!(
                    "[line {}] Error at 'break': Can't use 'break' outside of a loop.",
                    keyword.line
                ),
            ));
        }
    }

    fn visit_continue_stmt(&mut self, keyword: &Token) {
        if self.loop_depth == 0 {
            self.findings.push((
                keyword.line,
                format!(
                    "[line {}] Error at 'continue': Can't use 'continue' outside of a loop.",
                    keyword.line
                ),
            ));
        }
    }

    fn visit_class_stmt(&mut self, _name: &Token, _superclass: &Option<Expr>, methods: &[Stmt]) {
        for method in methods {
            if let Stmt::Function { body, .. } = method {
                let enclosing = self.loop_depth;
                self.loop_depth = 0;
                for stmt in body.iter() {
                    stmt.accept(self);
                }
                self.loop_depth = enclosing;
            }
        }
    }
}
