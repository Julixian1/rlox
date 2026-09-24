use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::token::Token;
use crate::value::Value; 

#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    values: HashMap<String, Value>,
    pub enclosing: Option<Rc<RefCell<Environment>>>,
}

impl Environment {
    pub fn new() -> Self {
        Environment {
            values: HashMap::new(),
            enclosing: None,
        }
    }

    pub fn new_enclosed(enclosing: Rc<RefCell<Environment>>) -> Self {
        Environment {
            values: HashMap::new(),
            enclosing: Some(enclosing),
        }
    }

    pub fn define(&mut self, name: String, value: Value) {
        self.values.insert(name, value);
    }

    pub fn get(&self, name: &Token) -> Result<Value, String> {
        if let Some(value) = self.values.get(&name.lexeme) {
            return Ok(value.clone());
        }

        if let Some(ref enclosing) = self.enclosing {
            return enclosing.borrow().get(name);
        }

        Err(format!(
            "[line {}] Error: Undefined variable '{}'.",
            name.line, name.lexeme
        ))
    }

    pub fn assign(&mut self, name: &Token, value: Value) -> Result<(), String> {
        if self.values.contains_key(&name.lexeme) {
            self.values.insert(name.lexeme.clone(), value);
            return Ok(());
        }

        if let Some(ref enclosing) = self.enclosing {
            return enclosing.borrow_mut().assign(name, value);
        }

        Err(format!(
            "[line {}] Error: Undefined variable '{}'.",
            name.line, name.lexeme
        ))
    }
}