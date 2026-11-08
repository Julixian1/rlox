use crate::chunk::{Chunk, OpCode};
use crate::value::Value;
use std::collections::HashMap;
use std::io::{self, Write};
 
pub struct Vm<W: Write> {
    stack: Vec<Value>,
    globals: HashMap<String, Value>,
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
            globals: HashMap::new(),
            out,
        }
    }
 
    pub fn into_output(self) -> W {
        self.out
    }
 
    pub fn interpret(&mut self, chunk: &Chunk) -> Result<(), String> {
        self.stack.clear();
        let result = self.run(chunk);
        if result.is_err() {
            self.stack.clear();
        }
        result
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
 
    fn run(&mut self, chunk: &Chunk) -> Result<(), String> {
        let mut ip = 0;
        loop {
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
 
                OpCode::GetLocal(slot) => {
                    let value = self.stack[slot as usize].clone();
                    self.stack.push(value);
                }
                OpCode::SetLocal(slot) => {
                    let value = self.peek().clone();
                    self.stack[slot as usize] = value;
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
 
                OpCode::Call(_) => {
                    return Err(Self::error(chunk, ip, "Can only call functions and classes."));
                }
                OpCode::Return => {
                    self.pop();
                    return Ok(());
                }
            }
        }
    }
}