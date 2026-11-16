//! Representación en tiempo de ejecución de clases, instancias y métodos ligados para Stack VM.

use crate::closure::Closure;
use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// Representación en tiempo de ejecución de una clase en la VM.
#[derive(Debug)]
pub struct VmClass {
    /// Nombre identificador de la clase.
    pub name: String,
    /// Métodos compilados registrados en la clase.
    pub methods: HashMap<String, Rc<Closure>>,
}

impl VmClass {
    /// Crea una nueva clase en la VM con el nombre indicado y sin métodos iniciales.
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
    /// Clase a partir de la cual fue instanciado el objeto.
    pub class: Rc<RefCell<VmClass>>,
    /// Campos y valores dinámicamente asignados a la instancia.
    pub fields: HashMap<String, Value>,
}

impl VmInstance {
    /// Crea una nueva instancia vinculada a la clase provista.
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
    /// Instancia receptora a la cual se vincula `this`.
    pub receiver: Value,
    /// Clausura del método ejecutable.
    pub method: Rc<Closure>,
}

impl BoundMethod {
    /// Crea un nuevo método ligado combinando un receptor y la clausura correspondiente.
    pub fn new(receiver: Value, method: Rc<Closure>) -> Self {
        BoundMethod { receiver, method }
    }
}

impl fmt::Display for BoundMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.method)
    }
}
