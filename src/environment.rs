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
    /// Crea un nuevo entorno global (raíz) sin ningún entorno envolvente.
    pub fn new() -> Self {
        Environment {
            values: HashMap::new(),
            enclosing: None,
        }
    }

    /// Crea un nuevo entorno local delimitado (enclosed) vinculado a un entorno padre.
    pub fn new_enclosed(enclosing: Rc<RefCell<Environment>>) -> Self {
        Environment {
            values: HashMap::new(),
            enclosing: Some(enclosing),
        }
    }

    /// Define una nueva variable en el entorno actual asociando su nombre a un valor.
    pub fn define(&mut self, name: String, value: Value) {
        self.values.insert(name, value);
    }

    /// Busca y obtiene el valor de una variable por su token en el entorno actual o en sus entornos padres.
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

    /// Busca y obtiene el valor de una variable dada una profundidad exacta (`depth`) en la cadena de alcances.
    pub fn get_at(&self, depth: usize, name: &str) -> Result<Value, String> {
        if depth == 0 {
            return self
                .values
                .get(name)
                .cloned()
                .ok_or_else(|| format!("Error: Undefined variable '{}' at depth 0.", name));
        }
        match &self.enclosing {
            Some(enclosing) => enclosing.borrow().get_at(depth - 1, name),
            None => Err(format!(
                "Error: Scope chain too shallow for variable '{}' (depth {}).",
                name, depth
            )),
        }
    }

    /// Reasigna el valor de una variable existente a una profundidad exacta (`depth`) en la cadena de alcances.
    pub fn assign_at(&mut self, depth: usize, name: &str, value: Value) -> Result<(), String> {
        if depth == 0 {
            if self.values.contains_key(name) {
                self.values.insert(name.to_string(), value);
                return Ok(());
            }
            return Err(format!("Error: Undefined variable '{}' at depth 0.", name));
        }
        match &self.enclosing {
            Some(enclosing) => enclosing.borrow_mut().assign_at(depth - 1, name, value),
            None => Err(format!(
                "Error: Scope chain too shallow for variable '{}' (depth {}).",
                name, depth
            )),
        }
    }

    /// Reasigna el valor de una variable existente en el entorno actual o en la cadena de entornos padres.
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
