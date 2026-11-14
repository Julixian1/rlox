use super::chunk::{RegChunk, RegOp};
use super::closure::{RegClosure, RegFrame, RegProto};
use crate::closure::{Upvalue, UpvalueRef};
use crate::value::Value;
use std::collections::HashMap;
use std::io::{self, Write};
use std::rc::Rc;

/// Mismo límite que la VM de pila: la recursión infinita da "Stack overflow."
const FRAMES_MAX: usize = 100_000;

pub struct RegVm<W: Write> {
    /// Los registros de un frame son `registers[base + r]`.
    registers: Vec<Value>,
    frames: Vec<RegFrame>,
    globals: HashMap<String, Value>,
    open_upvalues: Vec<UpvalueRef>,
    out: W,
}

impl RegVm<io::Stdout> {
    pub fn new() -> Self {
        Self::with_output(io::stdout())
    }
}

impl<W: Write> RegVm<W> {
    pub fn with_output(out: W) -> Self {
        RegVm {
            registers: Vec::new(),
            frames: Vec::new(),
            globals: HashMap::new(),
            open_upvalues: Vec::new(),
            out,
        }
    }

    pub fn into_output(self) -> W {
        self.out
    }

    pub fn interpret(&mut self, script: Rc<RegProto>) -> Result<(), String> {
        self.registers.clear();
        self.frames.clear();
        self.open_upvalues.clear();

        self.reserve(0, script.chunk.max_regs);
        let closure = Rc::new(RegClosure::new(script));
        self.frames.push(RegFrame::new(closure, 0));

        let result = self.run();
        if result.is_err() {
            self.registers.clear();
            self.frames.clear();
            self.open_upvalues.clear();
        }
        result
    }

    /// Asegura que existan los registros `base..base + regs`.
    fn reserve(&mut self, base: usize, regs: usize) {
        let needed = base + regs.max(1);
        if self.registers.len() < needed {
            self.registers.resize(needed, Value::Nil);
        }
    }

    fn name_at(chunk: &RegChunk, index: u16) -> &str {
        match &chunk.constants[index as usize] {
            Value::String(s) => s,
            other => unreachable!("el nombre de un global debe ser string, no {other:?}"),
        }
    }

    fn error(chunk: &RegChunk, ip: usize, msg: &str) -> String {
        format!("[line {}] {}", chunk.lines[ip - 1], msg)
    }

    /// Operación binaria sobre números: lee `b` y `c`, escribe el resultado en `a`.
    fn numeric(
        &mut self,
        chunk: &RegChunk,
        ip: usize,
        base: usize,
        (a, b, c): (u8, u8, u8),
        op: impl FnOnce(f64, f64) -> Value,
    ) -> Result<(), String> {
        let result = match (
            &self.registers[base + b as usize],
            &self.registers[base + c as usize],
        ) {
            (Value::Number(x), Value::Number(y)) => op(*x, *y),
            _ => return Err(Self::error(chunk, ip, "Operands must be numbers.")),
        };
        self.registers[base + a as usize] = result;
        Ok(())
    }

    // ---- upvalues -------------------------------------------------------

    fn open_index(upvalue: &UpvalueRef) -> usize {
        upvalue
            .borrow()
            .open_index()
            .expect("un upvalue cerrado no puede estar en la lista de abiertos")
    }

    /// Devuelve el upvalue abierto sobre `stack_index`, creándolo si no existe,
    /// para que dos closures que capturan la misma variable la compartan.
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

    /// Cierra los upvalues abiertos en `last` o más arriba.
    fn close_upvalues(&mut self, last: usize) {
        while self
            .open_upvalues
            .last()
            .is_some_and(|u| Self::open_index(u) >= last)
        {
            let upvalue = self.open_upvalues.pop().expect("recién comprobado");
            upvalue.borrow_mut().close(&self.registers);
        }
    }

    // ---- llamadas ---------------------------------------------------------

    /// Apila el frame de `closure`. Su registro 0 es `registers[callee + 1]`, o sea
    /// el primer argumento que dejó el llamador.
    fn call(&mut self, closure: Rc<RegClosure>, callee: usize, argc: usize) -> Result<(), String> {
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
        let base = callee + 1;
        self.reserve(base, closure.proto.chunk.max_regs);
        self.frames.push(RegFrame::new(closure, base));
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
                RegOp::LoadK(d, k) => {
                    self.registers[base + d as usize] = chunk.constants[k as usize].clone();
                }
                RegOp::LoadNil(d) => self.registers[base + d as usize] = Value::Nil,
                RegOp::LoadBool(d, b) => self.registers[base + d as usize] = Value::Boolean(b),
                RegOp::Move(d, s) => {
                    let value = self.registers[base + s as usize].clone();
                    self.registers[base + d as usize] = value;
                }

                RegOp::Add(d, b, c) => {
                    let result = match (
                        &self.registers[base + b as usize],
                        &self.registers[base + c as usize],
                    ) {
                        (Value::Number(x), Value::Number(y)) => Value::Number(x + y),
                        (Value::String(x), Value::String(y)) => {
                            let mut s = String::with_capacity(x.len() + y.len());
                            s.push_str(x);
                            s.push_str(y);
                            Value::String(s)
                        }
                        _ => {
                            return Err(Self::error(
                                chunk,
                                ip,
                                "Operands must be two numbers or two strings.",
                            ));
                        }
                    };
                    self.registers[base + d as usize] = result;
                }
                RegOp::Sub(d, b, c) => {
                    self.numeric(chunk, ip, base, (d, b, c), |x, y| Value::Number(x - y))?
                }
                RegOp::Mul(d, b, c) => {
                    self.numeric(chunk, ip, base, (d, b, c), |x, y| Value::Number(x * y))?
                }
                RegOp::Div(d, b, c) => {
                    self.numeric(chunk, ip, base, (d, b, c), |x, y| Value::Number(x / y))?
                }
                RegOp::Mod(d, b, c) => {
                    self.numeric(chunk, ip, base, (d, b, c), |x, y| Value::Number(x % y))?
                }

                RegOp::Neg(d, s) => {
                    let result = match &self.registers[base + s as usize] {
                        Value::Number(n) => Value::Number(-*n),
                        _ => return Err(Self::error(chunk, ip, "Operand must be a number.")),
                    };
                    self.registers[base + d as usize] = result;
                }
                RegOp::Not(d, s) => {
                    let result = Value::Boolean(!self.registers[base + s as usize].is_truthy());
                    self.registers[base + d as usize] = result;
                }

                RegOp::Eq(d, b, c) => {
                    let result = Value::Boolean(
                        self.registers[base + b as usize] == self.registers[base + c as usize],
                    );
                    self.registers[base + d as usize] = result;
                }
                RegOp::Lt(d, b, c) => {
                    self.numeric(chunk, ip, base, (d, b, c), |x, y| Value::Boolean(x < y))?
                }
                RegOp::Le(d, b, c) => {
                    self.numeric(chunk, ip, base, (d, b, c), |x, y| Value::Boolean(x <= y))?
                }

                RegOp::Jump(offset) => ip += offset as usize,
                RegOp::JumpIfFalse(r, offset) => {
                    if !self.registers[base + r as usize].is_truthy() {
                        ip += offset as usize;
                    }
                }
                RegOp::Loop(offset) => ip -= offset as usize,

                RegOp::GetGlobal(d, k) => {
                    let name = Self::name_at(chunk, k);
                    match self.globals.get(name).cloned() {
                        Some(value) => self.registers[base + d as usize] = value,
                        None => {
                            return Err(Self::error(
                                chunk,
                                ip,
                                &format!("Error: Undefined variable '{}'.", name),
                            ));
                        }
                    }
                }
                RegOp::SetGlobal(k, s) => {
                    let name = Self::name_at(chunk, k);
                    let value = self.registers[base + s as usize].clone();
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
                RegOp::DefineGlobal(k, s) => {
                    let value = self.registers[base + s as usize].clone();
                    self.globals
                        .insert(Self::name_at(chunk, k).to_string(), value);
                }

                RegOp::Call(a, argc) => {
                    let callee_reg = base + a as usize;
                    let callee = self.registers[callee_reg].clone();
                    let called = match callee {
                        Value::RegClosure(c) => self.call(c, callee_reg, argc as usize),
                        _ => Err("Can only call functions and classes.".to_string()),
                    };
                    called.map_err(|msg| Self::error(chunk, ip, &msg))?;

                    let caller = self.frames.len() - 2;
                    self.frames[caller].ip = ip;
                    let frame = self.frames.last().expect("no hay frame");
                    closure = Rc::clone(&frame.closure);
                    ip = frame.ip;
                    base = frame.base;
                }
                RegOp::Return(r) => {
                    let result = self.registers[base + r as usize].clone();
                    self.close_upvalues(base);
                    self.frames.pop();
                    match self.frames.last() {
                        None => return Ok(()),
                        Some(caller) => {
                            self.registers[base - 1] = result;
                            closure = Rc::clone(&caller.closure);
                            ip = caller.ip;
                            base = caller.base;
                        }
                    }
                }

                RegOp::Closure(d, k) => {
                    let proto = match &chunk.constants[k as usize] {
                        Value::RegProto(p) => Rc::clone(p),
                        other => unreachable!("Closure espera una función, no {other:?}"),
                    };
                    let mut new_closure = RegClosure::new(Rc::clone(&proto));
                    for desc in &proto.upvalues {
                        let upvalue = if desc.is_local {
                            self.capture_upvalue(base + desc.index as usize)
                        } else {
                            Rc::clone(&closure.upvalues[desc.index as usize])
                        };
                        new_closure.upvalues.push(upvalue);
                    }
                    self.registers[base + d as usize] = Value::RegClosure(Rc::new(new_closure));
                }
                RegOp::GetUpvalue(d, i) => {
                    let value = closure.upvalues[i as usize].borrow().get(&self.registers);
                    self.registers[base + d as usize] = value;
                }
                RegOp::SetUpvalue(i, s) => {
                    let value = self.registers[base + s as usize].clone();
                    closure.upvalues[i as usize]
                        .borrow_mut()
                        .set(&mut self.registers, value);
                }
                RegOp::Close(r) => self.close_upvalues(base + r as usize),

                RegOp::Print(r) => {
                    writeln!(self.out, "{}", self.registers[base + r as usize])
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
}
