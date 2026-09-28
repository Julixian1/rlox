use std::collections::HashMap;

use crate::expr::{Expr, ExprVisitor, LiteralValue};
use crate::stmt::{Stmt, StmtVisitor};
use crate::token::Token;

// Contexto donde aparece una función, para detectar `return` fuera de función.
#[derive(Clone, Copy, PartialEq)]
enum FunctionType {
    None,
    Function,
}

pub struct Resolver {
    // Pila de scopes locales. Cada scope es un mapa nombre -> ya_fue_inicializada.
    scopes: Vec<HashMap<String, bool>>,
    /// Mapa de profundidades resueltas: puntero al nodo Expr -> depth.
    /// El `Interpreter` consulta este mapa para hacer `get_at`/`assign_at`.
    pub locals: HashMap<*const Expr, usize>,
    current_function: FunctionType,
    pub errors: Vec<String>,
}

impl Resolver {
    /// Crea un nuevo `Resolver` con la pila de alcances vacía.
    pub fn new() -> Self {
        Resolver {
            scopes: Vec::new(),
            locals: HashMap::new(),
            current_function: FunctionType::None,
            errors: Vec::new(),
        }
    }

    /// Inicia un nuevo ámbito (scope) apilando un mapa de variables local.
    fn begin_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Finaliza el ámbito (scope) actual desapilando el mapa de variables local superior.
    fn end_scope(&mut self) {
        self.scopes.pop();
    }

    /// Declara una variable en el ámbito actual marcándola como aún no inicializada (`false`).
    fn declare(&mut self, name: &Token) {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(&name.lexeme) {
                self.errors.push(format!(
                    "[line {}] Error at '{}': Already a variable with this name in this scope.",
                    name.line, name.lexeme
                ));
            }
            scope.insert(name.lexeme.clone(), false);
        }
    }

    /// Define una variable en el ámbito actual marcándola como ya inicializada y lista para uso (`true`).
    fn define(&mut self, name: &Token) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.lexeme.clone(), true);
        }
    }

    /// Busca la variable de adentro hacia afuera (alcances internos a externos) y registra la distancia/profundidad en `locals`.
    fn resolve_local(&mut self, expr_ptr: *const Expr, name: &Token) {
        for (i, scope) in self.scopes.iter().rev().enumerate() {
            if scope.contains_key(&name.lexeme) {
                self.locals.insert(expr_ptr, i);
                return;
            }
        }
        // No encontrada en ningún scope local → es global, no se registra.
    }

    /// Resuelve el cuerpo de una función abriendo su propio scope e inicializando sus parámetros.
    fn resolve_function(&mut self, params: &[Token], body: &[Stmt], fn_type: FunctionType) {
        let enclosing = self.current_function;
        self.current_function = fn_type;

        self.begin_scope();
        for param in params {
            self.declare(param);
            self.define(param);
        }
        self.resolve_stmts(body);
        self.end_scope();

        self.current_function = enclosing;
    }

    /// Recorre y resuelve una lista de sentencias.
    pub fn resolve_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.resolve_stmt(stmt);
        }
    }

    /// Resuelve una sentencia individual despachando el Visitor.
    fn resolve_stmt(&mut self, stmt: &Stmt) {
        stmt.accept(self);
    }

    /// Resuelve una expresión registrando su puntero en el mapa de profundidades `locals`.
    pub fn resolve_expr(&mut self, expr: &Expr) {
        let ptr = expr as *const Expr;
        match expr {
            Expr::Variable { name } => {
                if let Some(scope) = self.scopes.last() {
                    if scope.get(&name.lexeme) == Some(&false) {
                        self.errors.push(format!(
                            "[line {}] Error at '{}': Can't read local variable in its own initializer.",
                            name.line, name.lexeme
                        ));
                    }
                }
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
        self.define(name);
    }

    fn visit_function_stmt(
        &mut self,
        name: &Token,
        params: &[Token],
        body: &std::rc::Rc<Vec<Stmt>>,
    ) {
        self.declare(name);
        self.define(name);
        self.resolve_function(params, body, FunctionType::Function);
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

    fn visit_return_stmt(&mut self, keyword: &Token, value: &Option<Expr>) {
        if self.current_function == FunctionType::None {
            self.errors.push(format!(
                "[line {}] Error at 'return': Can't return from top-level code.",
                keyword.line
            ));
        }
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
