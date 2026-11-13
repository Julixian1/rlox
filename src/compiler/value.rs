use crate::class::{LoxClass, LoxInstance};
use crate::closure::{Closure, FnProto};
use crate::expr::LiteralValue;
use crate::function::LoxFunction;
use crate::vm_class::{BoundMethod, VmClass, VmInstance};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
    Nil,
    Function(LoxFunction),
    Class(Rc<RefCell<LoxClass>>),
    Instance(Rc<RefCell<LoxInstance>>),
    Proto(Rc<FnProto>),
    Closure(Rc<Closure>),
    VmClass(Rc<RefCell<VmClass>>),
    VmInstance(Rc<RefCell<VmInstance>>),
    BoundMethod(Rc<BoundMethod>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::Function(a), Value::Function(b)) => a == b,
            (Value::Class(a), Value::Class(b)) => Rc::ptr_eq(a, b),
            (Value::Instance(a), Value::Instance(b)) => Rc::ptr_eq(a, b),
            (Value::Proto(a), Value::Proto(b)) => Rc::ptr_eq(a, b),
            (Value::Closure(a), Value::Closure(b)) => Rc::ptr_eq(a, b),
            (Value::VmClass(a), Value::VmClass(b)) => Rc::ptr_eq(a, b),
            (Value::VmInstance(a), Value::VmInstance(b)) => Rc::ptr_eq(a, b),
            (Value::BoundMethod(a), Value::BoundMethod(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Value {
    /// Determina si un valor de Lox se evalúa como verdadero (truthy).
    ///
    /// En Lox, únicamente `nil` y `false` son falsos; todos los demás valores son verdaderos.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Boolean(b) => *b,
            _ => true,
        }
    }
}

impl fmt::Display for Value {
    /// Formatea el valor para su representación textual en salida estándar (ej. en `print`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Nil => write!(f, "nil"),
            Value::Function(func) => write!(f, "<fn {}>", func.name()),
            Value::Class(klass) => write!(f, "{}", klass.borrow()),
            Value::Instance(instance) => write!(f, "{}", instance.borrow()),
            Value::Proto(proto) => write!(f, "{}", proto),
            Value::Closure(closure) => write!(f, "{}", closure),
            Value::VmClass(klass) => write!(f, "{}", klass.borrow()),
            Value::VmInstance(instance) => write!(f, "{}", instance.borrow()),
            Value::BoundMethod(bound) => write!(f, "{}", bound),
        }
    }
}

impl From<&LiteralValue> for Value {
    /// Convierte un `LiteralValue` del AST a un `Value` del entorno de ejecución.
    fn from(literal: &LiteralValue) -> Self {
        match literal {
            LiteralValue::Number(n) => Value::Number(*n),
            LiteralValue::StringVal(s) => Value::String(s.clone()),
            LiteralValue::Bool(b) => Value::Boolean(*b),
            LiteralValue::Nil => Value::Nil,
        }
    }
}
