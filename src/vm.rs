use crate::chunk::{Chunk, OpCode};
use crate::closure::{CallFrame, Closure, FnProto, Upvalue, UpvalueRef};
use crate::value::Value;
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
                    let callee = match callee {
                        Value::Closure(c) => c,
                        _ => {
                            return Err(Self::error(
                                chunk,
                                ip,
                                "Can only call functions and classes.",
                            ));
                        }
                    };
                    if callee.arity() != argc {
                        return Err(Self::error(
                            chunk,
                            ip,
                            &format!("Expected {} arguments but got {}.", callee.arity(), argc),
                        ));
                    }
                    if self.frames.len() >= FRAMES_MAX {
                        return Err(Self::error(chunk, ip, "Stack overflow."));
                    }
                    self.frames.last_mut().expect("no hay frame").ip = ip;
                    base = self.stack.len() - argc - 1;
                    self.frames.push(CallFrame::new(Rc::clone(&callee), base));
                    closure = callee;
                    ip = 0;
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
