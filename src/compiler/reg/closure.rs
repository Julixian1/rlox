//! Prototipos de función (`RegProto`), clausuras (`RegClosure`) y marcos de llamada (`RegFrame`) para la Register VM.

use super::chunk::RegChunk;
use crate::closure::{UpvalueDesc, UpvalueRef};
use std::fmt;
use std::rc::Rc;

/// Prototipo inmutable de función compilada para la máquina virtual de registros.
pub struct RegProto {
    /// Nombre opcional de la función (`None` para el script de nivel superior).
    pub name: Option<String>,
    /// Cantidad de parámetros formales esperados.
    pub arity: usize,
    /// Fragmento de código compilado asociado.
    pub chunk: RegChunk,
    /// Descriptores de variables capturadas (upvalues) por la función.
    pub upvalues: Vec<UpvalueDesc>,
}

impl RegProto {
    /// Crea un nuevo prototipo de función con el nombre opcional especificado.
    pub fn new(name: Option<String>) -> Self {
        RegProto {
            name,
            arity: 0,
            chunk: RegChunk::new(),
            upvalues: Vec::new(),
        }
    }

    /// Retorna la cantidad de upvalues capturados por este prototipo.
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

/// Clausura en tiempo de ejecución para la Register VM (prototipo junto con upvalues capturados).
pub struct RegClosure {
    /// Referencia compartida al prototipo inmutable de la función.
    pub proto: Rc<RegProto>,
    /// Lista de referencias a upvalues capturados en ejecución.
    pub upvalues: Vec<UpvalueRef>,
}

impl RegClosure {
    /// Instancia una nueva clausura a partir de su prototipo.
    pub fn new(proto: Rc<RegProto>) -> Self {
        let upvalues = Vec::with_capacity(proto.upvalue_count());
        RegClosure { proto, upvalues }
    }

    /// Retorna la aridad de la función consultando su prototipo.
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

/// Marco de llamada de función en la Register VM.
///
/// Los registros de la función se ubican en `stack[base + r]`. Al invocar, `base`
/// se sitúa sobre el registro del receptor, permitiendo recibir argumentos sin copia.
pub struct RegFrame {
    /// Clausura activa en este marco de llamada.
    pub closure: Rc<RegClosure>,
    /// Puntero de instrucción actual dentro del chunk.
    pub ip: usize,
    /// Índice base en el stack donde inician los registros virtuales del marco.
    pub base: usize,
}

impl RegFrame {
    /// Inicializa un marco de llamada con la clausura y posición base dadas.
    pub fn new(closure: Rc<RegClosure>, base: usize) -> Self {
        RegFrame {
            closure,
            ip: 0,
            base,
        }
    }
}
