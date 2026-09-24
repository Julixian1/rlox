use std::fmt;
use crate::expr::LiteralValue;
use crate::function::LoxFunction;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
    Nil,
    Function(LoxFunction),    
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Boolean(b) => *b,
            _ => true,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Nil => write!(f, "nil"),
            Value::Function(func) => write!(f, "<fn {}>", func.name()),
        }
    }
}

impl From<&LiteralValue> for Value {
    fn from(literal: &LiteralValue) -> Self {
        match literal {
            LiteralValue::Number(n) => Value::Number(*n),
            LiteralValue::StringVal(s) => Value::String(s.clone()),
            LiteralValue::Bool(b) => Value::Boolean(*b),
            LiteralValue::Nil => Value::Nil,
        }
    }
}
