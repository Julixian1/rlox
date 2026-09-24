use std::cell::RefCell;
use std::rc::Rc;

use crate::environment::Environment;
use crate::expr::Expr;
use crate::stmt::Stmt;
use crate::token::TokenType;
use crate::value::Value;
use crate::function::{LoxFunction, ReturnSignal};

pub struct Interpreter {
    environment: Rc<RefCell<Environment>>,
}

impl Interpreter {
    pub fn new() -> Self {
        Interpreter {
            environment: Rc::new(RefCell::new(Environment::new())),
        }
    }

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

    fn execute(&mut self, stmt: &Stmt) -> Result<(), ReturnSignal> {
        match stmt {
            Stmt::Expression { expression } => {
                self.evaluate(expression)?;
            }
            Stmt::Print { expression } => {
                let value = self.evaluate(expression)?;
                println!("{}", value);
            }
            Stmt::Var { name, initializer } => {
                let value = if let Some(init) = initializer {
                    self.evaluate(init)?
                } else {
                    Value::Nil
                };
                self.environment.borrow_mut().define(name.lexeme.clone(), value);
            }
            Stmt::Block { statements } => {
                let previous = self.environment.clone();
                let new_env = Rc::new(RefCell::new(Environment::new_enclosed(previous)));

                self.execute_block(statements, new_env)?;
            }
            Stmt::If { condition, then_branch, else_branch } => {
                if self.evaluate(condition)?.is_truthy() {
                    self.execute(then_branch)?;
                } else if let Some(else_stmt) = else_branch {
                    self.execute(else_stmt)?;
                }
            }
            Stmt::While { condition, body } => {
                while self.evaluate(condition)?.is_truthy() {
                    self.execute(body)?;
                }
            }
            Stmt::Function { name, params, body } => {
                let function = LoxFunction::new(
                    name.clone(),
                    params.clone(),
                    body.clone(),
                    self.environment.clone(), // Guarda el entorno actual como closure
                );
                self.environment
                    .borrow_mut()
                    .define(name.lexeme.clone(), Value::Function(function));
            }
            Stmt::Return { keyword: _, value } => {
                let return_val = if let Some(expr) = value {
                    self.evaluate(expr)?
                } else {
                    Value::Nil
                };

                return Err(ReturnSignal::Return(return_val));
            }
        }
        Ok(())
    }

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

    fn evaluate(&mut self, expr: &Expr) -> Result<Value, ReturnSignal> {
        match expr {
            Expr::Literal { value } => Ok(Value::from(value)),
            Expr::Grouping { expression } => self.evaluate(expression),
            Expr::Variable { name } => Ok(self.environment.borrow().get(name)?),
            Expr::Assign { name, value } => {
                let val = self.evaluate(value)?;
                self.environment.borrow_mut().assign(name, val.clone())?;
                Ok(val)
            }
            Expr::Unary { operator, right } => {
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
            Expr::Binary { left, operator, right } => {
                let left_val = self.evaluate(left)?;
                let right_val = self.evaluate(right)?;

                match operator.token_type {
                    TokenType::Plus => match (left_val, right_val) {
                        (Value::Number(l), Value::Number(r)) => Ok(Value::Number(l + r)),
                        (Value::String(l), Value::String(r)) => Ok(Value::String(format!("{}{}", l, r))),
                        _ => Err(ReturnSignal::Error(format!(
                            "[line {}] Operands must be two numbers or two strings.",
                            operator.line
                        ))),
                    },
                    TokenType::Minus => self.num_binary_op(left_val, right_val, operator.line, |a, b| a - b),
                    TokenType::Star => self.num_binary_op(left_val, right_val, operator.line, |a, b| a * b),
                    TokenType::Slash => self.num_binary_op(left_val, right_val, operator.line, |a, b| a / b),
                    TokenType::Greater => self.num_bool_op(left_val, right_val, operator.line, |a, b| a > b),
                    TokenType::GreaterEqual => self.num_bool_op(left_val, right_val, operator.line, |a, b| a >= b),
                    TokenType::Less => self.num_bool_op(left_val, right_val, operator.line, |a, b| a < b),
                    TokenType::LessEqual => self.num_bool_op(left_val, right_val, operator.line, |a, b| a <= b),
                    TokenType::EqualEqual => Ok(Value::Boolean(left_val == right_val)),
                    TokenType::BangEqual => Ok(Value::Boolean(left_val != right_val)),
                    TokenType::Percent => self.num_binary_op(left_val, right_val, operator.line, |a, b| a % b),
                    _ => Err(ReturnSignal::Error(format!(
                        "[line {}] Invalid binary operator.",
                        operator.line
                    ))),
                }
            }
            Expr::Logical { left, operator, right } => {
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
            Expr::Call { callee, paren, arguments } => {
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
    }

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