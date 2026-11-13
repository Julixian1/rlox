use crate::chunk::{Chunk, OpCode};
use crate::closure::{CallFrame, Closure, FnProto, Upvalue, UpvalueRef};
use crate::value::Value;
use crate::vm_class::{BoundMethod, VmClass, VmInstance};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Write};
use std::rc::Rc;

const FRAMES_MAX: usize = 100_000;

pub struct Vm<W: Write> {
    stack: Vec<Value>,
    frames: Vec<CallFrame>,
    globals: HashMap<String, Value>,
    open_upvalues: Vec<UpvalueRef>,
    out: W,
}

impl Vm<io::Stdout> {
    pub fn new() -> Self {
        Self::with_output(io::stdout())
    }
}

impl<W: Write> Vm<W> {
    pub fn with_output(out: W) -> Self {
        Vm {
            stack: Vec::with_capacity(256),
            frames: Vec::new(),
            globals: HashMap::new(),
            open_upvalues: Vec::new(),
            out,
        }
    }

    pub fn into_output(self) -> W {
        self.out
    }

    pub fn interpret(&mut self, script: Rc<FnProto>) -> Result<(), String> {
        self.reset();
        let closure = Rc::new(Closure::new(script));
        self.stack.push(Value::Closure(Rc::clone(&closure)));
        self.frames.push(CallFrame::new(closure, 0));
        let result = self.run();
        if result.is_err() {
            self.reset();
        }
        result
    }

    fn reset(&mut self) {
        self.stack.clear();
        self.frames.clear();
        self.open_upvalues.clear();
    }

    fn pop(&mut self) -> Value {
        self.stack
            .pop()
            .expect("stack underflow: el compiler emitió bytecode inválido")
    }

    fn peek(&self) -> &Value {
        self.stack
            .last()
            .expect("stack vacío: el compiler emitió bytecode inválido")
    }

    fn name_at(chunk: &Chunk, index: u16) -> &str {
        match &chunk.constants[index as usize] {
            Value::String(s) => s,
            other => unreachable!("el nombre de un global debe ser string, no {other:?}"),
        }
    }

    fn error(chunk: &Chunk, ip: usize, msg: &str) -> String {
        format!("[line {}] {}", chunk.lines[ip - 1], msg)
    }

    fn numeric(
        &mut self,
        chunk: &Chunk,
        ip: usize,
        op: impl FnOnce(f64, f64) -> Value,
    ) -> Result<(), String> {
        let b = self.pop();
        let a = self.pop();
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                self.stack.push(op(x, y));
                Ok(())
            }
            _ => Err(Self::error(chunk, ip, "Operands must be numbers.")),
        }
    }

    fn open_index(upvalue: &UpvalueRef) -> usize {
        upvalue
            .borrow()
            .open_index()
            .expect("un upvalue cerrado no puede estar en la lista de abiertos")
    }

    fn capture_upvalue(&mut self, stack_index: usize) -> UpvalueRef {
        match self
            .open_upvalues
            .binary_search_by_key(&stack_index, Self::open_index)
        {
            Ok(i) => Rc::clone(&self.open_upvalues[i]),
            Err(i) => {
                let upvalue = Upvalue::new_open(stack_index);
                self.open_upvalues.insert(i, Rc::clone(&upvalue));
                upvalue
            }
        }
    }

    fn close_upvalues(&mut self, last: usize) {
        while self
            .open_upvalues
            .last()
            .is_some_and(|u| Self::open_index(u) >= last)
        {
            let upvalue = self.open_upvalues.pop().expect("recién comprobado");
            upvalue.borrow_mut().close(&self.stack);
        }
    }

    fn call_value(&mut self, callee: Value, argc: usize) -> Result<(), String> {
        match callee {
            Value::Closure(closure) => self.call_closure(closure, argc),
            Value::BoundMethod(bound) => {
                let slot = self.stack.len() - 1 - argc;
                self.stack[slot] = bound.receiver.clone();
                self.call_closure(Rc::clone(&bound.method), argc)
            }
            Value::VmClass(klass) => {
                let instance = Rc::new(RefCell::new(VmInstance::new(klass.clone())));
                let slot = self.stack.len() - 1 - argc;
                self.stack[slot] = Value::VmInstance(instance);

                if let Some(init) = klass.borrow().methods.get("init").cloned() {
                    self.call_closure(init, argc)
                } else if argc != 0 {
                    Err(format!("Expected 0 arguments but got {}.", argc))
                } else {
                    Ok(())
                }
            }
            _ => Err("Can only call functions and classes.".to_string()),
        }
    }

    fn call_closure(&mut self, closure: Rc<Closure>, argc: usize) -> Result<(), String> {
        if closure.arity() != argc {
            return Err(format!(
                "Expected {} arguments but got {}.",
                closure.arity(),
                argc
            ));
        }
        if self.frames.len() >= FRAMES_MAX {
            return Err("Stack overflow.".to_string());
        }
        let base = self.stack.len() - argc - 1;
        self.frames.push(CallFrame::new(closure, base));
        Ok(())
    }

    fn run(&mut self) -> Result<(), String> {
        let mut closure = Rc::clone(&self.frames.last().expect("no hay frame").closure);
        let mut ip = 0;
        let mut base = 0;

        loop {
            let chunk = &closure.proto.chunk;
            let op = chunk.code[ip];
            ip += 1;
            match op {
                OpCode::Constant(k) => self.stack.push(chunk.constants[k as usize].clone()),
                OpCode::Nil => self.stack.push(Value::Nil),
                OpCode::True => self.stack.push(Value::Boolean(true)),
                OpCode::False => self.stack.push(Value::Boolean(false)),
                OpCode::Pop => {
                    self.pop();
                }

                // Los slots son relativos a la base del frame actual.
                OpCode::GetLocal(slot) => {
                    let value = self.stack[base + slot as usize].clone();
                    self.stack.push(value);
                }
                OpCode::SetLocal(slot) => {
                    let value = self.peek().clone();
                    self.stack[base + slot as usize] = value;
                }
                OpCode::DefineGlobal(k) => {
                    let value = self.pop();
                    self.globals
                        .insert(Self::name_at(chunk, k).to_string(), value);
                }
                OpCode::GetGlobal(k) => {
                    let name = Self::name_at(chunk, k);
                    match self.globals.get(name).cloned() {
                        Some(value) => self.stack.push(value),
                        None => {
                            return Err(Self::error(
                                chunk,
                                ip,
                                &format!("Error: Undefined variable '{}'.", name),
                            ));
                        }
                    }
                }
                OpCode::SetGlobal(k) => {
                    let name = Self::name_at(chunk, k);
                    let value = self.peek().clone();
                    match self.globals.get_mut(name) {
                        Some(slot) => *slot = value,
                        None => {
                            return Err(Self::error(
                                chunk,
                                ip,
                                &format!("Error: Undefined variable '{}'.", name),
                            ));
                        }
                    }
                }
                OpCode::GetUpvalue(i) => {
                    let value = closure.upvalues[i as usize].borrow().get(&self.stack);
                    self.stack.push(value);
                }
                OpCode::SetUpvalue(i) => {
                    let value = self.peek().clone();
                    closure.upvalues[i as usize]
                        .borrow_mut()
                        .set(&mut self.stack, value);
                }

                OpCode::Equal => {
                    let b = self.pop();
                    let a = self.pop();
                    self.stack.push(Value::Boolean(a == b));
                }
                OpCode::Greater => self.numeric(chunk, ip, |a, b| Value::Boolean(a > b))?,
                OpCode::Less => self.numeric(chunk, ip, |a, b| Value::Boolean(a < b))?,
                OpCode::Add => {
                    let b = self.pop();
                    let a = self.pop();
                    match (a, b) {
                        (Value::Number(x), Value::Number(y)) => {
                            self.stack.push(Value::Number(x + y))
                        }
                        (Value::String(mut x), Value::String(y)) => {
                            x.push_str(&y);
                            self.stack.push(Value::String(x));
                        }
                        _ => {
                            return Err(Self::error(
                                chunk,
                                ip,
                                "Operands must be two numbers or two strings.",
                            ));
                        }
                    }
                }
                OpCode::Subtract => self.numeric(chunk, ip, |a, b| Value::Number(a - b))?,
                OpCode::Multiply => self.numeric(chunk, ip, |a, b| Value::Number(a * b))?,
                OpCode::Divide => self.numeric(chunk, ip, |a, b| Value::Number(a / b))?,
                OpCode::Modulo => self.numeric(chunk, ip, |a, b| Value::Number(a % b))?,
                OpCode::Not => {
                    let value = self.pop();
                    self.stack.push(Value::Boolean(!value.is_truthy()));
                }
                OpCode::Negate => match self.stack.last_mut() {
                    Some(Value::Number(n)) => *n = -*n,
                    _ => return Err(Self::error(chunk, ip, "Operand must be a number.")),
                },

                OpCode::Print => {
                    let value = self.pop();
                    writeln!(self.out, "{}", value).map_err(|e| e.to_string())?;
                }
                OpCode::Jump(offset) => ip += offset as usize,
                OpCode::JumpIfFalse(offset) => {
                    if !self.peek().is_truthy() {
                        ip += offset as usize;
                    }
                }
                OpCode::Loop(offset) => ip -= offset as usize,

                OpCode::Call(argc) => {
                    let argc = argc as usize;
                    // El callee está debajo de sus argumentos.
                    let callee = self.stack[self.stack.len() - 1 - argc].clone();
                    self.frames.last_mut().expect("no hay frame").ip = ip;
                    self.call_value(callee, argc)
                        .map_err(|msg| Self::error(chunk, ip, &msg))?;
                    let frame = self.frames.last().expect("no hay frame");
                    closure = Rc::clone(&frame.closure);
                    ip = frame.ip;
                    base = frame.base;
                }
                OpCode::Closure(k) => {
                    let proto = match &chunk.constants[k as usize] {
                        Value::Proto(p) => Rc::clone(p),
                        other => unreachable!("OP_CLOSURE espera una función, no {other:?}"),
                    };
                    let slot = self.stack.len();
                    self.stack.push(Value::Nil);

                    let mut upvalues = Vec::with_capacity(proto.upvalue_count());
                    for desc in &proto.upvalues {
                        let upvalue = if desc.is_local {
                            self.capture_upvalue(base + desc.index as usize)
                        } else {
                            Rc::clone(&closure.upvalues[desc.index as usize])
                        };
                        upvalues.push(upvalue);
                    }
                    self.stack[slot] = Value::Closure(Rc::new(Closure { proto, upvalues }));
                }
                OpCode::CloseUpvalue => {
                    self.close_upvalues(self.stack.len() - 1);
                    self.pop();
                }
                OpCode::Class(k) => {
                    let name = Self::name_at(chunk, k);
                    let klass = VmClass::new(name.to_string());
                    self.stack
                        .push(Value::VmClass(Rc::new(RefCell::new(klass))));
                }
                OpCode::Method(k) => {
                    let name = Self::name_at(chunk, k);
                    let method = match self.pop() {
                        Value::Closure(c) => c,
                        _ => unreachable!("OP_METHOD espera una closure"),
                    };
                    match self.peek() {
                        Value::VmClass(klass) => {
                            klass.borrow_mut().methods.insert(name.to_string(), method);
                        }
                        _ => unreachable!("OP_METHOD espera una clase en el tope del stack"),
                    }
                }
                OpCode::CheckInstance => match self.peek() {
                    Value::VmInstance(_) => {}
                    _ => return Err(Self::error(chunk, ip, "Only instances have fields.")),
                },
                OpCode::GetProperty(k) => {
                    let name = Self::name_at(chunk, k);
                    let obj = self.pop();
                    match obj {
                        Value::VmInstance(instance) => {
                            if let Some(val) = instance.borrow().fields.get(name) {
                                self.stack.push(val.clone());
                            } else {
                                let method_opt = {
                                    let class = Rc::clone(&instance.borrow().class);
                                    class.borrow().methods.get(name).cloned()
                                };
                                if let Some(method) = method_opt {
                                    let bound =
                                        BoundMethod::new(Value::VmInstance(instance), method);
                                    self.stack.push(Value::BoundMethod(Rc::new(bound)));
                                } else {
                                    return Err(Self::error(
                                        chunk,
                                        ip,
                                        &format!("Undefined property '{}'.", name),
                                    ));
                                }
                            }
                        }
                        _ => return Err(Self::error(chunk, ip, "Only instances have properties.")),
                    }
                }
                OpCode::SetProperty(k) => {
                    let name = Self::name_at(chunk, k);
                    let value = self.pop();
                    let obj = self.pop();
                    match obj {
                        Value::VmInstance(instance) => {
                            instance
                                .borrow_mut()
                                .fields
                                .insert(name.to_string(), value.clone());
                            self.stack.push(value);
                        }
                        _ => return Err(Self::error(chunk, ip, "Only instances have fields.")),
                    }
                }
                OpCode::Inherit => {
                    let len = self.stack.len();
                    if len < 2 {
                        return Err(Self::error(chunk, ip, "Stack underflow in OP_INHERIT."));
                    }
                    let superclass_val = self.stack[len - 2].clone();
                    let subclass_val = self.stack[len - 1].clone();

                    let superclass = match superclass_val {
                        Value::VmClass(ref k) => Rc::clone(k),
                        _ => return Err(Self::error(chunk, ip, "Superclass must be a class.")),
                    };
                    let subclass = match subclass_val {
                        Value::VmClass(ref k) => Rc::clone(k),
                        _ => return Err(Self::error(chunk, ip, "Subclass must be a class.")),
                    };

                    let methods_to_copy: Vec<(String, Rc<Closure>)> = superclass
                        .borrow()
                        .methods
                        .iter()
                        .map(|(k, v)| (k.clone(), Rc::clone(v)))
                        .collect();

                    for (name, method) in methods_to_copy {
                        subclass.borrow_mut().methods.insert(name, method);
                    }

                    self.pop();
                }
                OpCode::GetSuper(k) => {
                    let name = Self::name_at(chunk, k);
                    let superclass_val = self.pop();
                    let this_val = self.pop();

                    let superclass = match superclass_val {
                        Value::VmClass(ref k) => Rc::clone(k),
                        _ => unreachable!("OP_GET_SUPER espera una clase"),
                    };

                    if let Some(method) = superclass.borrow().methods.get(name).cloned() {
                        let bound = BoundMethod::new(this_val, method);
                        self.stack.push(Value::BoundMethod(Rc::new(bound)));
                    } else {
                        return Err(Self::error(
                            chunk,
                            ip,
                            &format!("Undefined property '{}'.", name),
                        ));
                    }
                }
                OpCode::Return => {
                    let result = self.pop();
                    let frame = self.frames.pop().expect("no hay frame");
                    self.close_upvalues(frame.base);
                    self.stack.truncate(frame.base);

                    let caller = match self.frames.last() {
                        Some(caller) => caller,
                        None => return Ok(()),
                    };
                    self.stack.push(result);
                    closure = Rc::clone(&caller.closure);
                    ip = caller.ip;
                    base = caller.base;
                }
            }
        }
    }
}
