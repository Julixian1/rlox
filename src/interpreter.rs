use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::environment::Environment;
use crate::expr::{Expr, ExprVisitor, LiteralValue};
use crate::stmt::{Stmt, StmtVisitor};
use crate::token::{Token, TokenType};
use crate::value::Value;
use crate::function::{LoxFunction, ReturnSignal};

pub struct Interpreter {
    pub globals: Rc<RefCell<Environment>>,
    environment: Rc<RefCell<Environment>>,
    locals: HashMap<*const Expr, usize>,
}

unsafe impl Send for Interpreter {}

impl Interpreter {
    /// Crea un nuevo `Interpreter` inicializando los entornos globales e individuales.
    pub fn new() -> Self {
        let globals = Rc::new(RefCell::new(Environment::new()));
        Interpreter {
            globals: globals.clone(),
            environment: globals,
            locals: HashMap::new(),
        }
    }

    /// Llamado por el Resolver para registrar la profundidad de un nodo Expr en la pila de ámbitos.
    pub fn resolve(&mut self, expr: *const Expr, depth: usize) {
        self.locals.insert(expr, depth);
    }

    /// Interpreta y ejecuta una secuencia de sentencias del programa Lox.
    pub fn interpret(&mut self, statements: &[Stmt]) -> Result<(), String> {
        for stmt in statements {
            if let Err(err) = self.execute(stmt) {
                return match err {
                    ReturnSignal::Error(msg) => Err(msg),
                    ReturnSignal::Return(_) => Err("Cannot return from top-level code.".to_string()),
                };
            }
        }
        Ok(())
    }

    /// Ejecuta una sentencia individual del AST delegando a `accept(self)`.
    pub fn execute(&mut self, stmt: &Stmt) -> Result<(), ReturnSignal> {
        stmt.accept(self)
    }

    /// Evalúa un nodo de expresión del AST y devuelve su valor resultante (`Value`).
    pub fn evaluate(&mut self, expr: &Expr) -> Result<Value, ReturnSignal> {
        let ptr = expr as *const Expr;
        match expr {
            Expr::Variable { name } => {
                self.lookup_variable(ptr, name)
            }
            Expr::Assign { name, value } => {
                let val = self.evaluate(value)?;
                if let Some(&depth) = self.locals.get(&ptr) {
                    self.environment
                        .borrow_mut()
                        .assign_at(depth, &name.lexeme, val.clone())
                        .map_err(ReturnSignal::Error)?;
                } else {
                    self.environment
                        .borrow_mut()
                        .assign(name, val.clone())
                        .map_err(ReturnSignal::Error)?;
                }
                Ok(val)
            }
            _ => expr.accept(self),
        }
    }

    /// Busca una variable: usa `get_at` si la variable fue resuelta localmente a cierta profundidad, o busca en `globals`.
    fn lookup_variable(&self, ptr: *const Expr, name: &Token) -> Result<Value, ReturnSignal> {
        if let Some(&depth) = self.locals.get(&ptr) {
            self.environment
                .borrow()
                .get_at(depth, &name.lexeme)
                .map_err(ReturnSignal::Error)
        } else {
            self.globals
                .borrow()
                .get(name)
                .map_err(ReturnSignal::Error)
        }
    }

    /// Ejecuta un bloque de sentencias dentro del entorno (environment) proporcionado y restaura el entorno previo al finalizar.
    pub fn execute_block(
        &mut self,
        statements: &[Stmt],
        environment: Rc<RefCell<Environment>>,
    ) -> Result<(), ReturnSignal> {
        let previous = self.environment.clone();
        self.environment = environment;

        let mut result = Ok(());
        for statement in statements {
            if let Err(err) = self.execute(statement) {
                result = Err(err);
                break;
            }
        }

        self.environment = previous;
        result
    }

    /// Auxiliar para aplicar operaciones binarias numéricas (ej. resta, multiplicación, división).
    fn num_binary_op(
        &self,
        left: Value,
        right: Value,
        line: usize,
        op: impl FnOnce(f64, f64) -> f64,
    ) -> Result<Value, ReturnSignal> {
        match (left, right) {
            (Value::Number(l), Value::Number(r)) => Ok(Value::Number(op(l, r))),
            _ => Err(ReturnSignal::Error(format!("[line {}] Operands must be numbers.", line))),
        }
    }

    /// Auxiliar para aplicar comparaciones relacionales sobre números produciendo un `Value::Boolean`.
    fn num_bool_op(
        &self,
        left: Value,
        right: Value,
        line: usize,
        op: impl FnOnce(f64, f64) -> bool,
    ) -> Result<Value, ReturnSignal> {
        match (left, right) {
            (Value::Number(l), Value::Number(r)) => Ok(Value::Boolean(op(l, r))),
            _ => Err(ReturnSignal::Error(format!("[line {}] Operands must be numbers.", line))),
        }
    }
}


impl StmtVisitor<Result<(), ReturnSignal>> for Interpreter {
    fn visit_expression_stmt(&mut self, expression: &Expr) -> Result<(), ReturnSignal> {
        self.evaluate(expression)?;
        Ok(())
    }

    fn visit_print_stmt(&mut self, expression: &Expr) -> Result<(), ReturnSignal> {
        let value = self.evaluate(expression)?;
        println!("{}", value);
        Ok(())
    }

    fn visit_var_stmt(
        &mut self,
        name: &Token,
        initializer: &Option<Expr>,
    ) -> Result<(), ReturnSignal> {
        let value = if let Some(init) = initializer {
            self.evaluate(init)?
        } else {
            Value::Nil
        };
        self.environment
            .borrow_mut()
            .define(name.lexeme.clone(), value);
        Ok(())
    }

    fn visit_block_stmt(&mut self, statements: &[Stmt]) -> Result<(), ReturnSignal> {
        let previous = self.environment.clone();
        let new_env = Rc::new(RefCell::new(Environment::new_enclosed(previous)));
        self.execute_block(statements, new_env)
    }

    fn visit_if_stmt(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: &Option<Box<Stmt>>,
    ) -> Result<(), ReturnSignal> {
        if self.evaluate(condition)?.is_truthy() {
            self.execute(then_branch)?;
        } else if let Some(else_stmt) = else_branch {
            self.execute(else_stmt)?;
        }
        Ok(())
    }

    fn visit_while_stmt(&mut self, condition: &Expr, body: &Stmt) -> Result<(), ReturnSignal> {
        while self.evaluate(condition)?.is_truthy() {
            self.execute(body)?;
        }
        Ok(())
    }

    fn visit_function_stmt(
        &mut self,
        name: &Token,
        params: &[Token],
        body: &Rc<Vec<Stmt>>,
    ) -> Result<(), ReturnSignal> {
        let function = LoxFunction::new(
            name.clone(),
            params.to_vec(),
            body.clone(),
            self.environment.clone(),
        );
        self.environment
            .borrow_mut()
            .define(name.lexeme.clone(), Value::Function(function));
        Ok(())
    }

    fn visit_return_stmt(
        &mut self,
        _keyword: &Token,
        value: &Option<Expr>,
    ) -> Result<(), ReturnSignal> {
        let return_val = if let Some(expr) = value {
            self.evaluate(expr)?
        } else {
            Value::Nil
        };

        Err(ReturnSignal::Return(return_val))
    }
}


impl ExprVisitor<Result<Value, ReturnSignal>> for Interpreter {
    fn visit_literal_expr(&mut self, value: &LiteralValue) -> Result<Value, ReturnSignal> {
        Ok(Value::from(value))
    }

    fn visit_grouping_expr(&mut self, expression: &Expr) -> Result<Value, ReturnSignal> {
        self.evaluate(expression)
    }

    fn visit_variable_expr(&mut self, name: &Token) -> Result<Value, ReturnSignal> {
        self.environment.borrow().get(name).map_err(ReturnSignal::Error)
    }

    fn visit_assign_expr(&mut self, name: &Token, value: &Expr) -> Result<Value, ReturnSignal> {
        let val = self.evaluate(value)?;
        self.environment
            .borrow_mut()
            .assign(name, val.clone())
            .map_err(ReturnSignal::Error)?;
        Ok(val)
    }

    fn visit_unary_expr(
        &mut self,
        operator: &Token,
        right: &Expr,
    ) -> Result<Value, ReturnSignal> {
        let right_val = self.evaluate(right)?;
        match operator.token_type {
            TokenType::Minus => match right_val {
                Value::Number(n) => Ok(Value::Number(-n)),
                _ => Err(ReturnSignal::Error(format!(
                    "[line {}] Operand must be a number.",
                    operator.line
                ))),
            },
            TokenType::Bang => Ok(Value::Boolean(!right_val.is_truthy())),
            _ => Err(ReturnSignal::Error(format!(
                "[line {}] Unknown unary operator.",
                operator.line
            ))),
        }
    }

    fn visit_binary_expr(
        &mut self,
        left: &Expr,
        operator: &Token,
        right: &Expr,
    ) -> Result<Value, ReturnSignal> {
        let left_val = self.evaluate(left)?;
        let right_val = self.evaluate(right)?;

        match operator.token_type {
            TokenType::Plus => match (left_val, right_val) {
                (Value::Number(l), Value::Number(r)) => Ok(Value::Number(l + r)),
                (Value::String(l), Value::String(r)) => {
                    Ok(Value::String(format!("{}{}", l, r)))
                }
                _ => Err(ReturnSignal::Error(format!(
                    "[line {}] Operands must be two numbers or two strings.",
                    operator.line
                ))),
            },
            TokenType::Minus => { self.num_binary_op(left_val, right_val, operator.line, |a, b| a - b)}
            TokenType::Star  => { self.num_binary_op(left_val, right_val, operator.line, |a, b| a * b)}
            TokenType::Slash => { self.num_binary_op(left_val, right_val, operator.line, |a, b| a / b)}
            TokenType::Greater      => { self.num_bool_op(left_val, right_val, operator.line, |a, b| a > b)}
            TokenType::GreaterEqual => { self.num_bool_op(left_val, right_val, operator.line, |a, b| a >= b)}
            TokenType::Less         => { self.num_bool_op(left_val, right_val, operator.line, |a, b| a < b)}
            TokenType::LessEqual    => { self.num_bool_op(left_val, right_val, operator.line, |a, b| a <= b)}
            TokenType::EqualEqual   => Ok(Value::Boolean(left_val == right_val)),
            TokenType::BangEqual    => Ok(Value::Boolean(left_val != right_val)),
            TokenType::Percent => { self.num_binary_op(left_val, right_val, operator.line, |a, b| a % b)}
            _ => Err(ReturnSignal::Error(format!(
                "[line {}] Invalid binary operator.",
                operator.line
            ))),
        }
    }

    fn visit_logical_expr(
        &mut self,
        left: &Expr,
        operator: &Token,
        right: &Expr,
    ) -> Result<Value, ReturnSignal> {
        let left_val = self.evaluate(left)?;
        if operator.token_type == TokenType::Or {
            if left_val.is_truthy() {
                return Ok(left_val);
            }
        } else {
            if !left_val.is_truthy() {
                return Ok(left_val);
            }
        }
        self.evaluate(right)
    }

    fn visit_call_expr(
        &mut self,
        callee: &Expr,
        paren: &Token,
        arguments: &[Expr],
    ) -> Result<Value, ReturnSignal> {
        let callee_val = self.evaluate(callee)?;

        let mut args_val = Vec::new();
        for arg in arguments {
            args_val.push(self.evaluate(arg)?);
        }

        if let Value::Function(function) = callee_val {
            if args_val.len() != function.arity() {
                return Err(ReturnSignal::Error(format!(
                    "[line {}] Expected {} arguments but got {}.",
                    paren.line,
                    function.arity(),
                    args_val.len()
                )));
            }

            function.call(self, args_val)
        } else {
            Err(ReturnSignal::Error(format!(
                "[line {}] Can only call functions and classes.",
                paren.line
            )))
        }
    }
}