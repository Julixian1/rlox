use crate::chunk::Chunk;
use crate::value::Value;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
 
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpvalueDesc {
    pub is_local: bool,
    pub index: u16,
}
 
pub struct FnProto {
    pub name: Option<String>,
    pub arity: usize,
    pub chunk: Chunk,
    pub upvalues: Vec<UpvalueDesc>,
}
 
impl FnProto {
    pub fn new(name: Option<String>) -> Self {
        FnProto {
            name,
            arity: 0,
            chunk: Chunk::new(),
            upvalues: Vec::new(),
        }
    }
 
    pub fn script() -> Self {
        Self::new(None)
    }
 
    pub fn is_script(&self) -> bool {
        self.name.is_none()
    }
 
    pub fn upvalue_count(&self) -> usize {
        self.upvalues.len()
    }
 
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
 
#[derive(Debug, Clone, PartialEq)]
pub enum Upvalue {
    Open(usize),
    Closed(Value),
}
 
pub type UpvalueRef = Rc<RefCell<Upvalue>>;
 
impl Upvalue {
    pub fn new_open(stack_index: usize) -> UpvalueRef {
        Rc::new(RefCell::new(Upvalue::Open(stack_index)))
    }
 
    pub fn open_index(&self) -> Option<usize> {
        match self {
            Upvalue::Open(i) => Some(*i),
            Upvalue::Closed(_) => None,
        }
    }
 
    pub fn get(&self, stack: &[Value]) -> Value {
        match self {
            Upvalue::Open(i) => stack[*i].clone(),
            Upvalue::Closed(v) => v.clone(),
        }
    }
 
    pub fn set(&mut self, stack: &mut [Value], value: Value) {
        match self {
            Upvalue::Open(i) => stack[*i] = value,
            Upvalue::Closed(v) => *v = value,
        }
    }
 
    pub fn close(&mut self, stack: &[Value]) {
        if let Upvalue::Open(i) = self {
            *self = Upvalue::Closed(stack[*i].clone());
        }
    }
}
 
pub struct Closure {
    pub proto: Rc<FnProto>,
    pub upvalues: Vec<UpvalueRef>,
}
 
impl Closure {
    pub fn new(proto: Rc<FnProto>) -> Self {
        let upvalues = Vec::with_capacity(proto.upvalue_count());
        Closure { proto, upvalues }
    }
 
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
 
pub struct CallFrame {
    pub closure: Rc<Closure>,
    pub ip: usize,
    pub base: usize,
}
 
impl CallFrame {
    pub fn new(closure: Rc<Closure>, base: usize) -> Self {
        CallFrame {
            closure,
            ip: 0,
            base,
        }
    }
}