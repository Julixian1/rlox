use crate::closure::Closure;
use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// Representación en tiempo de ejecución de una clase en la VM.
#[derive(Debug)]
pub struct VmClass {
    pub name: String,
    pub methods: HashMap<String, Rc<Closure>>,
}

impl VmClass {
    pub fn new(name: String) -> Self {
        VmClass {
            name,
            methods: HashMap::new(),
        }
    }
}

impl fmt::Display for VmClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<class {}>", self.name)
    }
}

/// Representación en tiempo de ejecución de una instancia de clase en la VM.
#[derive(Debug)]
pub struct VmInstance {
    pub class: Rc<RefCell<VmClass>>,
    pub fields: HashMap<String, Value>,
}

impl VmInstance {
    pub fn new(class: Rc<RefCell<VmClass>>) -> Self {
        VmInstance {
            class,
            fields: HashMap::new(),
        }
    }
}

impl fmt::Display for VmInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<instance of {}>", self.class.borrow().name)
    }
}

/// Representación de un método ligado a una instancia receptora en la VM.
#[derive(Debug)]
pub struct BoundMethod {
    pub receiver: Value,
    pub method: Rc<Closure>,
}

impl BoundMethod {
    pub fn new(receiver: Value, method: Rc<Closure>) -> Self {
        BoundMethod { receiver, method }
    }
}

impl fmt::Display for BoundMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.method)
    }
}
