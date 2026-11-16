//! Representación universal de valores en tiempo de ejecución para los tres backends de Lox.

use crate::class::{LoxClass, LoxInstance};
use crate::closure::{Closure, FnProto};
use crate::compiler::reg::closure::{RegClosure, RegProto};
use crate::expr::LiteralValue;
use crate::function::LoxFunction;
use crate::vm_class::{BoundMethod, VmClass, VmInstance};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// Puntero a función nativa invocable desde Lox con argumentos de tipo `Value`.
pub type NativeFnPtr = fn(&[Value]) -> Result<Value, String>;

/// Descriptor de una función nativa del sistema o funciones integradas (built-ins).
#[derive(Clone)]
pub struct NativeFn {
    /// Nombre identificador de la función nativa.
    pub name: String,
    /// Aridad esperada por la función nativa.
    pub arity: usize,
    /// Puntero a la función en Rust.
    pub func: NativeFnPtr,
}

impl fmt::Debug for NativeFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<native fn {}>", self.name)
    }
}

impl PartialEq for NativeFn {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.arity == other.arity
            && (self.func as usize == other.func as usize)
    }
}

/// Valida y convierte un `Value` numérico a un índice entero válido dentro de los límites de una lista.
pub fn validate_list_index(list_len: usize, index_val: &Value) -> Result<usize, String> {
    match index_val {
        Value::Number(n) => {
            if n.fract() != 0.0 {
                return Err("Index must be an integer.".to_string());
            }
            if *n < 0.0 {
                return Err("Index cannot be negative.".to_string());
            }
            let idx = *n as usize;
            if idx >= list_len {
                return Err(format!("Index out of bounds: {} >= len {}.", idx, list_len));
            }
            Ok(idx)
        }
        _ => Err("Index must be a number.".to_string()),
    }
}

/// Función nativa `len`: retorna la cantidad de elementos de una lista dada.
pub fn native_len(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err(format!("Expected 1 argument but got {}.", args.len()));
    }
    match &args[0] {
        Value::List(l) => Ok(Value::Number(l.borrow().len() as f64)),
        _ => Err("Argument to 'len' must be a list.".to_string()),
    }
}

/// Función nativa `push`: agrega un elemento al final de una lista mutable.
pub fn native_push(args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err(format!("Expected 2 arguments but got {}.", args.len()));
    }
    match &args[0] {
        Value::List(l) => {
            l.borrow_mut().push(args[1].clone());
            Ok(Value::Nil)
        }
        _ => Err("First argument to 'push' must be a list.".to_string()),
    }
}

/// Función nativa `pop`: extrae y retorna el último elemento de una lista mutable.
pub fn native_pop(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err(format!("Expected 1 argument but got {}.", args.len()));
    }
    match &args[0] {
        Value::List(l) => {
            let mut list = l.borrow_mut();
            if list.is_empty() {
                return Err("Cannot pop from an empty list.".to_string());
            }
            Ok(list.pop().unwrap())
        }
        _ => Err("Argument to 'pop' must be a list.".to_string()),
    }
}

/// Representación de cualquier valor computable en Lox (primitivos, referencias y objetos invocables).
#[derive(Debug, Clone)]
pub enum Value {
    /// Valor numérico de coma flotante de doble precisión (f64).
    Number(f64),
    /// Cadena de texto dinámica.
    String(String),
    /// Valor booleano (`true` o `false`).
    Boolean(bool),
    /// Ausencia de valor o nulo (`nil`).
    Nil,
    /// Función definida por el usuario para el backend Tree-Walk.
    Function(Rc<LoxFunction>),
    /// Definición de clase para el backend Tree-Walk.
    Class(Rc<RefCell<LoxClass>>),
    /// Instancia de clase para el backend Tree-Walk.
    Instance(Rc<RefCell<LoxInstance>>),
    /// Prototipo de función para Stack VM.
    Proto(Rc<FnProto>),
    /// Clausura en tiempo de ejecución para Stack VM.
    Closure(Rc<Closure>),
    /// Definición de clase optimizada para Stack y Register VM.
    VmClass(Rc<RefCell<VmClass>>),
    /// Instancia de clase optimizada para Stack y Register VM.
    VmInstance(Rc<RefCell<VmInstance>>),
    /// Método enlazado a una instancia para Stack y Register VM.
    BoundMethod(Rc<BoundMethod>),
    /// Prototipo de función para Register VM.
    RegProto(Rc<RegProto>),
    /// Clausura en tiempo de ejecución para Register VM.
    RegClosure(Rc<RegClosure>),
    /// Lista nativa indexable y mutable.
    List(Rc<RefCell<Vec<Value>>>),
    /// Función nativa del sistema.
    NativeFn(Rc<NativeFn>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b) || a == b,
            (Value::Class(a), Value::Class(b)) => Rc::ptr_eq(a, b),
            (Value::Instance(a), Value::Instance(b)) => Rc::ptr_eq(a, b),
            (Value::Proto(a), Value::Proto(b)) => Rc::ptr_eq(a, b),
            (Value::Closure(a), Value::Closure(b)) => Rc::ptr_eq(a, b),
            (Value::VmClass(a), Value::VmClass(b)) => Rc::ptr_eq(a, b),
            (Value::VmInstance(a), Value::VmInstance(b)) => Rc::ptr_eq(a, b),
            (Value::BoundMethod(a), Value::BoundMethod(b)) => Rc::ptr_eq(a, b),
            (Value::RegProto(a), Value::RegProto(b)) => Rc::ptr_eq(a, b),
            (Value::RegClosure(a), Value::RegClosure(b)) => Rc::ptr_eq(a, b),
            (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
            (Value::NativeFn(a), Value::NativeFn(b)) => Rc::ptr_eq(a, b) || a == b,
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

impl Value {
    /// Formatea recursivamente el valor rastreando referencias cíclicas a listas para evitar desbordes de pila.
    fn fmt_with_visited(
        &self,
        f: &mut fmt::Formatter<'_>,
        visited: &mut Vec<*const RefCell<Vec<Value>>>,
    ) -> fmt::Result {
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
            Value::RegProto(proto) => write!(f, "{}", proto),
            Value::RegClosure(closure) => write!(f, "{}", closure),
            Value::List(list) => {
                let ptr = Rc::as_ptr(list);
                if visited.contains(&ptr) {
                    return write!(f, "[...]");
                }
                visited.push(ptr);
                let elements = list.borrow();
                write!(f, "[")?;
                for (i, el) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    el.fmt_with_visited(f, visited)?;
                }
                write!(f, "]")?;
                drop(elements);
                visited.pop();
                Ok(())
            }
            Value::NativeFn(nf) => write!(f, "<native fn {}>", nf.name),
        }
    }
}

impl fmt::Display for Value {
    /// Formatea el valor para su representación textual en salida estándar (ej. en `print`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut visited = Vec::new();
        self.fmt_with_visited(f, &mut visited)
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
