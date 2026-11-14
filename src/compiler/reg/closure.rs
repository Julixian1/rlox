use super::chunk::RegChunk;
use crate::closure::{UpvalueDesc, UpvalueRef};
use std::fmt;
use std::rc::Rc;

/// Función compilada a registros: es lo que el codegen guarda como constante.
pub struct RegProto {
    pub name: Option<String>,
    pub arity: usize,
    pub chunk: RegChunk,
    pub upvalues: Vec<UpvalueDesc>,
}

impl RegProto {
    pub fn new(name: Option<String>) -> Self {
        RegProto {
            name,
            arity: 0,
            chunk: RegChunk::new(),
            upvalues: Vec::new(),
        }
    }

    pub fn upvalue_count(&self) -> usize {
        self.upvalues.len()
    }
}

impl fmt::Display for RegProto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => write!(f, "<fn {}>", name),
            None => write!(f, "<script>"),
        }
    }
}

impl fmt::Debug for RegProto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl PartialEq for RegProto {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Una función en ejecución: su prototipo más las variables que capturó.
pub struct RegClosure {
    pub proto: Rc<RegProto>,
    pub upvalues: Vec<UpvalueRef>,
}

impl RegClosure {
    pub fn new(proto: Rc<RegProto>) -> Self {
        let upvalues = Vec::with_capacity(proto.upvalue_count());
        RegClosure { proto, upvalues }
    }

    pub fn arity(&self) -> usize {
        self.proto.arity
    }
}

impl fmt::Display for RegClosure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.proto)
    }
}

impl fmt::Debug for RegClosure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.proto)
    }
}

impl PartialEq for RegClosure {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Los registros de la función son `stack[base + r]`. Al llamar, `base`
/// queda justo encima del registro del callee, así que los argumentos que
/// dejó el llamador pasan a ser los primeros registros del frame sin copiarse.
pub struct RegFrame {
    pub closure: Rc<RegClosure>,
    pub ip: usize,
    pub base: usize,
}

impl RegFrame {
    pub fn new(closure: Rc<RegClosure>, base: usize) -> Self {
        RegFrame {
            closure,
            ip: 0,
            base,
        }
    }
}
