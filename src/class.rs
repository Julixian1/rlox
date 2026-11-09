use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::function::LoxFunction;
use crate::value::Value;

/// Representa una clase definida por el usuario en Lox.
#[derive(Clone)]
pub struct LoxClass {
    pub name: String,
    pub superclass: Option<Rc<RefCell<LoxClass>>>,
    pub methods: HashMap<String, LoxFunction>,
}

impl LoxClass {
    /// Crea una nueva clase con su nombre, superclase opcional y métodos.
    pub fn new(
        name: String,
        superclass: Option<Rc<RefCell<LoxClass>>>,
        methods: HashMap<String, LoxFunction>,
    ) -> Self {
        LoxClass {
            name,
            superclass,
            methods,
        }
    }

    /// Busca un método en la clase actual; si no existe, busca recursivamente en la superclase.
    pub fn find_method(&self, name: &str) -> Option<LoxFunction> {
        if let Some(method) = self.methods.get(name) {
            return Some(method.clone());
        }
        if let Some(ref superclass) = self.superclass {
            return superclass.borrow().find_method(name);
        }
        None
    }

    /// Aridad de la clase al ser llamada como constructora.
    /// Si existe 'init' (propio o heredado en la cadena), devuelve su aridad; de lo contrario 0.
    pub fn arity(&self) -> usize {
        self.find_method("init")
            .map(|init| init.arity())
            .unwrap_or(0)
    }
}

impl fmt::Display for LoxClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<class {}>", self.name)
    }
}

impl fmt::Debug for LoxClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<class {}>", self.name)
    }
}

/// Representa una instancia concreta de una clase Lox en tiempo de ejecución.
#[derive(Clone)]
pub struct LoxInstance {
    pub class: Rc<RefCell<LoxClass>>,
    pub fields: HashMap<String, Value>,
}

impl LoxInstance {
    /// Crea una nueva instancia de la clase dada.
    pub fn new(class: Rc<RefCell<LoxClass>>) -> Self {
        LoxInstance {
            class,
            fields: HashMap::new(),
        }
    }

    /// Obtiene una propiedad de la instancia: primero busca en los campos, luego en los métodos.
    pub fn get(instance: &Rc<RefCell<LoxInstance>>, name: &str) -> Option<Value> {
        if let Some(value) = instance.borrow().fields.get(name) {
            return Some(value.clone());
        }
        let class = instance.borrow().class.clone();
        if let Some(method) = class.borrow().find_method(name) {
            let bound = method.bind(Rc::clone(instance));
            return Some(Value::Function(bound));
        }
        None
    }

    /// Asigna o actualiza el valor de un campo en la instancia.
    pub fn set(&mut self, name: String, value: Value) {
        self.fields.insert(name, value);
    }
}

impl fmt::Display for LoxInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<instance of {}>", self.class.borrow().name)
    }
}

impl fmt::Debug for LoxInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<instance of {}>", self.class.borrow().name)
    }
}

