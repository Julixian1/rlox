//! Prototipos de función (`FnProto`), clausuras (`Closure`), upvalues y marcos de llamada (`CallFrame`) para Stack VM.

use crate::chunk::Chunk;
use crate::value::Value;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_PROTO_ID: AtomicUsize = AtomicUsize::new(1);

/// Descriptor estático de una variable capturada (upvalue) por una función.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpvalueDesc {
    /// Determina si la variable capturada es local a la función directamente contenedora.
    pub is_local: bool,
    /// Índice del registro/slot local o del upvalue de la función contenedora.
    pub index: u16,
}

/// Prototipo inmutable de una función compilada para la Stack VM.
#[derive(Clone)]
pub struct FnProto {
    /// Identificador único del prototipo de función.
    pub id: usize,
    /// Nombre de la función, o `None` si representa el script principal.
    pub name: Option<String>,
    /// Cantidad de parámetros formales requeridos.
    pub arity: usize,
    /// Bloque de bytecode de la función.
    pub chunk: Chunk,
    /// Lista de descriptores de upvalues que captura la función.
    pub upvalues: Vec<UpvalueDesc>,
}

impl FnProto {
    /// Crea un nuevo prototipo de función con el nombre provisto y un chunk vacío.
    pub fn new(name: Option<String>) -> Self {
        FnProto {
            id: NEXT_PROTO_ID.fetch_add(1, Ordering::Relaxed),
            name,
            arity: 0,
            chunk: Chunk::new(),
            upvalues: Vec::new(),
        }
    }

    /// Crea un prototipo para el script principal de nivel superior.
    pub fn script() -> Self {
        Self::new(None)
    }

    /// Comprueba si el prototipo corresponde al script de nivel superior.
    pub fn is_script(&self) -> bool {
        self.name.is_none()
    }

    /// Retorna la cantidad de upvalues capturados por la función.
    pub fn upvalue_count(&self) -> usize {
        self.upvalues.len()
    }

    /// Agrega un descriptor de upvalue si no fue agregado previamente y retorna su índice.
    pub fn add_upvalue(&mut self, desc: UpvalueDesc) -> usize {
        match self.upvalues.iter().position(|u| *u == desc) {
            Some(i) => i,
            None => {
                self.upvalues.push(desc);
                self.upvalues.len() - 1
            }
        }
    }
}

impl fmt::Display for FnProto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => write!(f, "<fn {}>", name),
            None => write!(f, "<script>"),
        }
    }
}

impl fmt::Debug for FnProto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl PartialEq for FnProto {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Estado en tiempo de ejecución de una variable capturada por una clausura.
#[derive(Debug, Clone, PartialEq)]
pub enum Upvalue {
    /// Upvalue abierto que aún reside en la pila de ejecución en la posición indicada.
    Open(usize),
    /// Upvalue cerrado cuyo valor fue copiado al heap al abandonar el ámbito de la variable.
    Closed(Value),
}

/// Referencia compartida mutable a un upvalue en tiempo de ejecución.
pub type UpvalueRef = Rc<RefCell<Upvalue>>;

impl Upvalue {
    /// Crea una nueva referencia a un upvalue abierto en la posición de pila especificada.
    pub fn new_open(stack_index: usize) -> UpvalueRef {
        Rc::new(RefCell::new(Upvalue::Open(stack_index)))
    }

    /// Retorna el índice de la pila si el upvalue continúa abierto.
    pub fn open_index(&self) -> Option<usize> {
        match self {
            Upvalue::Open(i) => Some(*i),
            Upvalue::Closed(_) => None,
        }
    }

    /// Lee el valor actual del upvalue desde la pila o desde su copia cerrada.
    pub fn get(&self, stack: &[Value]) -> Value {
        match self {
            Upvalue::Open(i) => stack[*i].clone(),
            Upvalue::Closed(v) => v.clone(),
        }
    }

    /// Modifica el valor de la variable capturada en la pila o en su copia cerrada.
    pub fn set(&mut self, stack: &mut [Value], value: Value) {
        match self {
            Upvalue::Open(i) => stack[*i] = value,
            Upvalue::Closed(v) => *v = value,
        }
    }

    /// Cierra el upvalue copiando el valor correspondiente de la pila a su estado interno cerrado.
    pub fn close(&mut self, stack: &[Value]) {
        if let Upvalue::Open(i) = self {
            *self = Upvalue::Closed(stack[*i].clone());
        }
    }
}

/// Instancia de función en ejecución que encapsula su prototipo y sus upvalues capturados.
pub struct Closure {
    /// Prototipo de la función compilada.
    pub proto: Rc<FnProto>,
    /// Vector de referencias compartidas a los upvalues capturados.
    pub upvalues: Vec<UpvalueRef>,
}

impl Closure {
    /// Crea una nueva clausura para el prototipo indicado, asignando espacio para sus upvalues.
    pub fn new(proto: Rc<FnProto>) -> Self {
        let upvalues = Vec::with_capacity(proto.upvalue_count());
        Closure { proto, upvalues }
    }

    /// Retorna la aridad del prototipo de función subyacente.
    pub fn arity(&self) -> usize {
        self.proto.arity
    }
}

impl fmt::Display for Closure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.proto)
    }
}

impl fmt::Debug for Closure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.proto)
    }
}

impl PartialEq for Closure {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Marco de llamada activo en la pila de llamadas de la Stack VM.
pub struct CallFrame {
    /// Clausura que se está ejecutando en este marco.
    pub closure: Rc<Closure>,
    /// Puntero de instrucción actual dentro del chunk.
    pub ip: usize,
    /// Posición base del marco en la pila de valores de la VM.
    pub base: usize,
    /// Desplazamiento inicial en la tabla de cachés en línea para este marco.
    pub ic_base: usize,
}

impl CallFrame {
    /// Crea un nuevo marco de ejecución inicializado con `ip = 0`.
    pub fn new(closure: Rc<Closure>, base: usize, ic_base: usize) -> Self {
        CallFrame {
            closure,
            ip: 0,
            base,
            ic_base,
        }
    }
}
