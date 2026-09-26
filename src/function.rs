use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::environment::Environment;
use crate::interpreter::Interpreter;
use crate::stmt::Stmt;
use crate::token::Token;
use crate::value::Value;

#[derive(Debug)]
pub enum ReturnSignal {
    Return(Value),
    Error(String),
}

impl From<String> for ReturnSignal {
    fn from(err: String) -> Self {
        ReturnSignal::Error(err)
    }
}

#[derive(Clone)]
pub struct LoxFunction {
    pub name: Token,
    pub params: Vec<Token>,
    pub body: Rc<Vec<Stmt>>,
    pub closure: Rc<RefCell<Environment>>, 
}

impl LoxFunction {
    pub fn new(name: Token, params: Vec<Token>, body: Rc<Vec<Stmt>>, closure: Rc<RefCell<Environment>>) -> Self {
        LoxFunction {
            name,
            params,
            body,
            closure,
        }
    }

    pub fn name(&self) -> &str {
        &self.name.lexeme
    }

    pub fn arity(&self) -> usize {
        self.params.len()
    }

    pub fn call(&self, interpreter: &mut Interpreter, arguments: Vec<Value>) -> Result<Value, ReturnSignal> {
        let environment = Rc::new(RefCell::new(Environment::new_enclosed(self.closure.clone())));

        for (param, arg) in self.params.iter().zip(arguments) {
            environment.borrow_mut().define(param.lexeme.clone(), arg);
        }

        let result = interpreter.execute_block(&self.body, environment);

        match result {
            Err(ReturnSignal::Return(value)) => Ok(value),
            Err(ReturnSignal::Error(err)) => Err(ReturnSignal::Error(err)),
            Ok(_) => Ok(Value::Nil),
        }
    }
}

impl fmt::Debug for LoxFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<fn {}>", self.name.lexeme)
    }
}

impl PartialEq for LoxFunction {
    fn eq(&self, other: &Self) -> bool {
        self.name.lexeme == other.name.lexeme
    }
}