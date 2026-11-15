//! Máquina virtual basada en pila (Stack VM) y bucle de interpretación de bytecode.

use crate::chunk::{Chunk, OpCode};
use crate::closure::{CallFrame, Closure, FnProto, Upvalue, UpvalueRef};
use crate::value::Value;
use crate::vm_class::{BoundMethod, VmClass, VmInstance};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Write};
use std::rc::Rc;

const FRAMES_MAX: usize = 100_000;

/// Objeto invocable resuelto de forma anticipada para optimización de llamadas de métodos o campos.
#[derive(Debug, Clone)]
enum PreparedCallable {
    /// Método de clase optimizado.
    Method(Rc<Closure>),
    /// Campo o propiedad que almacena un valor invocable.
    Field(Value),
}

/// Estado de invocación preparado pendiente de ejecutarse con sus argumentos.
#[derive(Debug, Clone)]
struct PreparedCall {
    /// Invocable resuelto (método o campo).
    callable: PreparedCallable,
    /// Profundidad del marco de llamada al momento de la preparación.
    frame_depth: usize,
    /// Ranura de pila del receptor (`this`).
    receiver_slot: usize,
}

/// Máquina virtual basada en pila para la ejecución de bytecode de Lox.
pub struct Vm<W: Write> {
    /// Pila de operandos y variables locales.
    stack: Vec<Value>,
    /// Pila de marcos de llamada de funciones (`CallFrame`).
    frames: Vec<CallFrame>,
    /// Tabla de nombres de variables globales mapeadas a índices en `global_values`.
    global_names: HashMap<String, usize>,
    /// Valores de variables globales indexadas.
    global_values: Vec<Option<Value>>,
    /// Desplazamientos base para Inline Caches por identificador de prototipo.
    proto_ic_offsets: HashMap<usize, usize>,
    /// Ranuras de almacenamiento de Inline Caches.
    ic_slots: Vec<u32>,
    /// Lista de upvalues abiertos que apuntan a ranuras de la pila.
    open_upvalues: Vec<UpvalueRef>,
    /// Pila de llamadas a métodos preparadas pendientes de ejecución.
    prepared_calls: Vec<PreparedCall>,
    /// Flujo de salida donde se dirigen las escrituras (ej. `print`).
    out: W,
}

impl Vm<io::Stdout> {
    /// Crea una nueva máquina virtual con salida estándar (`io::stdout`).
    pub fn new() -> Self {
        Self::with_output(io::stdout())
    }
}

impl<W: Write> Vm<W> {
    /// Crea una nueva máquina virtual con un flujo de salida configurable e inicializa las funciones nativas.
    pub fn with_output(out: W) -> Self {
        let mut vm = Vm {
            stack: Vec::with_capacity(256),
            frames: Vec::with_capacity(64),
            global_names: HashMap::new(),
            global_values: Vec::new(),
            proto_ic_offsets: HashMap::new(),
            ic_slots: Vec::new(),
            open_upvalues: Vec::new(),
            prepared_calls: Vec::with_capacity(16),
            out,
        };
        let len_slot = vm.global_values.len();
        vm.global_names.insert("len".to_string(), len_slot);
        vm.global_values.push(Some(Value::NativeFn(std::rc::Rc::new(
            crate::value::NativeFn {
                name: "len".to_string(),
                arity: 1,
                func: crate::value::native_len,
            },
        ))));
        let push_slot = vm.global_values.len();
        vm.global_names.insert("push".to_string(), push_slot);
        vm.global_values.push(Some(Value::NativeFn(std::rc::Rc::new(
            crate::value::NativeFn {
                name: "push".to_string(),
                arity: 2,
                func: crate::value::native_push,
            },
        ))));
        let pop_slot = vm.global_values.len();
        vm.global_names.insert("pop".to_string(), pop_slot);
        vm.global_values.push(Some(Value::NativeFn(std::rc::Rc::new(
            crate::value::NativeFn {
                name: "pop".to_string(),
                arity: 1,
                func: crate::value::native_pop,
            },
        ))));
        let pop_slot = vm.global_values.len();
        vm.global_names.insert("pop".to_string(), pop_slot);
        vm.global_values.push(Some(Value::NativeFn(std::rc::Rc::new(crate::value::NativeFn {
            name: "pop".to_string(),
            arity: 1,
            func: crate::value::native_pop,
        }))));
        vm
    }

    /// Consume la máquina virtual retornando el flujo de salida subyacente.
    pub fn into_output(self) -> W {
        self.out
    }

    #[inline(always)]
    fn get_or_init_ic_base(&mut self, proto: &FnProto) -> usize {
        let proto_id = proto.id;
        if let Some(&offset) = self.proto_ic_offsets.get(&proto_id) {
            offset
        } else {
            let offset = self.ic_slots.len();
            let count = proto.chunk.constants.len();
            self.ic_slots.resize(offset + count, u32::MAX);
            self.proto_ic_offsets.insert(proto_id, offset);
            offset
        }
    }

    /// Interpreta un prototipo compilado ejecutándolo hasta su finalización.
    pub fn interpret(&mut self, script: Rc<FnProto>) -> Result<(), String> {
        self.reset();
        let closure = Rc::new(Closure::new(script));
        let ic_base = self.get_or_init_ic_base(&closure.proto);
        self.stack.push(Value::Closure(Rc::clone(&closure)));
        self.frames.push(CallFrame::new(closure, 0, ic_base));
        let result = self.run();
        if result.is_err() {
            self.reset();
        } else {
            debug_assert!(
                self.prepared_calls.is_empty(),
                "prepared_calls no quedó vacía tras la ejecución exitosa"
            );
        }
        result
    }

    fn reset(&mut self) {
        self.stack.clear();
        self.frames.clear();
        self.open_upvalues.clear();
        self.prepared_calls.clear();
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

    #[inline(always)]
    fn binary_num<F>(&mut self, chunk: &Chunk, ip: usize, op: F) -> Result<(), String>
    where
        F: FnOnce(f64, f64) -> f64,
    {
        let len = self.stack.len();
        let (x, y) = match (&self.stack[len - 2], &self.stack[len - 1]) {
            (Value::Number(x), Value::Number(y)) => (*x, *y),
            _ => return Err(Self::error(chunk, ip, "Operands must be numbers.")),
        };
        self.stack[len - 2] = Value::Number(op(x, y));
        self.stack.pop();
        Ok(())
    }

    #[inline(always)]
    fn binary_num_bool<F>(&mut self, chunk: &Chunk, ip: usize, op: F) -> Result<(), String>
    where
        F: FnOnce(f64, f64) -> bool,
    {
        let len = self.stack.len();
        let (x, y) = match (&self.stack[len - 2], &self.stack[len - 1]) {
            (Value::Number(x), Value::Number(y)) => (*x, *y),
            _ => return Err(Self::error(chunk, ip, "Operands must be numbers.")),
        };
        self.stack[len - 2] = Value::Boolean(op(x, y));
        self.stack.pop();
        Ok(())
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

    #[inline(always)]
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
        let ic_base = self.get_or_init_ic_base(&closure.proto);
        self.frames.push(CallFrame::new(closure, base, ic_base));
        Ok(())
    }

    /// Bucle principal de ejecución que despacha e interpreta las instrucciones de bytecode.
    fn run(&mut self) -> Result<(), String> {
        let mut closure = Rc::clone(&self.frames.last().expect("no hay frame").closure);
        let mut ip = 0;
        let mut base = 0;
        let mut ic_base = self.frames.last().expect("no hay frame").ic_base;

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
                OpCode::Dup => {
                    let value = self.peek().clone();
                    self.stack.push(value);
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
                OpCode::SetLocalPop(slot) => {
                    let value = self.pop();
                    self.stack[base + slot as usize] = value;
                }
                OpCode::DefineGlobal(k) => {
                    let value = self.pop();
                    let name = Self::name_at(chunk, k);
                    if let Some(&slot) = self.global_names.get(name) {
                        self.global_values[slot] = Some(value);
                    } else {
                        let slot = self.global_values.len();
                        self.global_values.push(Some(value));
                        self.global_names.insert(name.to_string(), slot);
                    }
                }
                OpCode::GetGlobal(k) => {
                    let name = Self::name_at(chunk, k);
                    let val_opt = self
                        .global_names
                        .get(name)
                        .and_then(|&slot| self.global_values.get(slot).and_then(|v| v.as_ref()));
                    match val_opt {
                        Some(value) => self.stack.push(value.clone()),
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
                    let slot_opt = self.global_names.get(name).copied();
                    match slot_opt {
                        Some(slot)
                            if self
                                .global_values
                                .get(slot)
                                .and_then(|v| v.as_ref())
                                .is_some() =>
                        {
                            self.global_values[slot] = Some(value);
                        }
                        _ => {
                            return Err(Self::error(
                                chunk,
                                ip,
                                &format!("Error: Undefined variable '{}'.", name),
                            ));
                        }
                    }
                }
                OpCode::GetGlobalFast(k) => {
                    let ic_idx = ic_base + k as usize;
                    let slot = self.ic_slots[ic_idx];
                    if slot != u32::MAX {
                        if let Some(ref val) = self.global_values[slot as usize] {
                            self.stack.push(val.clone());
                            continue;
                        }
                    }

                    let name = Self::name_at(chunk, k);
                    match self.global_names.get(name).copied() {
                        Some(slot)
                            if self
                                .global_values
                                .get(slot)
                                .and_then(|v| v.as_ref())
                                .is_some() =>
                        {
                            self.ic_slots[ic_idx] = slot as u32;
                            self.stack
                                .push(self.global_values[slot].as_ref().unwrap().clone());
                        }
                        _ => {
                            return Err(Self::error(
                                chunk,
                                ip,
                                &format!("Error: Undefined variable '{}'.", name),
                            ));
                        }
                    }
                }
                OpCode::SetGlobalFast(k) => {
                    let value = self.peek().clone();
                    let ic_idx = ic_base + k as usize;
                    let slot = self.ic_slots[ic_idx];
                    if slot != u32::MAX {
                        if self
                            .global_values
                            .get(slot as usize)
                            .and_then(|v| v.as_ref())
                            .is_some()
                        {
                            self.global_values[slot as usize] = Some(value);
                            continue;
                        }
                    }

                    let name = Self::name_at(chunk, k);
                    match self.global_names.get(name).copied() {
                        Some(slot)
                            if self
                                .global_values
                                .get(slot)
                                .and_then(|v| v.as_ref())
                                .is_some() =>
                        {
                            self.ic_slots[ic_idx] = slot as u32;
                            self.global_values[slot] = Some(value);
                        }
                        _ => {
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
                    let len = self.stack.len();
                    let eq = self.stack[len - 2] == self.stack[len - 1];
                    self.stack[len - 2] = Value::Boolean(eq);
                    self.stack.pop();
                }
                OpCode::Greater => self.binary_num_bool(chunk, ip, |a, b| a > b)?,
                OpCode::Less => self.binary_num_bool(chunk, ip, |a, b| a < b)?,
                OpCode::GreaterEqual => self.binary_num_bool(chunk, ip, |a, b| a >= b)?,
                OpCode::LessEqual => self.binary_num_bool(chunk, ip, |a, b| a <= b)?,
                OpCode::Add => {
                    let len = self.stack.len();
                    match (&self.stack[len - 2], &self.stack[len - 1]) {
                        (Value::Number(x), Value::Number(y)) => {
                            let res = x + y;
                            self.stack[len - 2] = Value::Number(res);
                            self.stack.pop();
                        }
                        (Value::String(_), Value::String(_)) => {
                            let b = self.pop();
                            let a = self.pop();
                            match (a, b) {
                                (Value::String(mut x), Value::String(y)) => {
                                    x.push_str(&y);
                                    self.stack.push(Value::String(x));
                                }
                                _ => unreachable!(),
                            }
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
                OpCode::Subtract => self.binary_num(chunk, ip, |a, b| a - b)?,
                OpCode::Multiply => self.binary_num(chunk, ip, |a, b| a * b)?,
                OpCode::Divide => self.binary_num(chunk, ip, |a, b| a / b)?,
                OpCode::Modulo => self.binary_num(chunk, ip, |a, b| a % b)?,
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
                    let callee_slot = self.stack.len() - 1 - argc;
                    self.frames.last_mut().expect("no hay frame").ip = ip;

                    match &self.stack[callee_slot] {
                        Value::Closure(c) => {
                            let c = Rc::clone(c);
                            self.call_closure(Rc::clone(&c), argc)
                                .map_err(|msg| Self::error(chunk, ip, &msg))?;
                            closure = c;
                            ip = 0;
                            base = callee_slot;
                            ic_base = self.frames.last().expect("no hay frame").ic_base;
                        }
                        Value::BoundMethod(bound) => {
                            let receiver = bound.receiver.clone();
                            let method = Rc::clone(&bound.method);
                            self.stack[callee_slot] = receiver;
                            self.call_closure(Rc::clone(&method), argc)
                                .map_err(|msg| Self::error(chunk, ip, &msg))?;
                            closure = method;
                            ip = 0;
                            base = callee_slot;
                            ic_base = self.frames.last().expect("no hay frame").ic_base;
                        }
                        Value::VmClass(klass) => {
                            let klass = Rc::clone(klass);
                            let instance =
                                Rc::new(RefCell::new(VmInstance::new(Rc::clone(&klass))));
                            self.stack[callee_slot] = Value::VmInstance(instance);

                            let init = klass.borrow().methods.get("init").cloned();
                            if let Some(init) = init {
                                self.call_closure(Rc::clone(&init), argc)
                                    .map_err(|msg| Self::error(chunk, ip, &msg))?;
                                closure = init;
                                ip = 0;
                                base = callee_slot;
                                ic_base = self.frames.last().expect("no hay frame").ic_base;
                            } else if argc != 0 {
                                return Err(Self::error(
                                    chunk,
                                    ip,
                                    &format!("Expected 0 arguments but got {}.", argc),
                                ));
                            }
                        }
                        Value::NativeFn(native) => {
                            if argc != native.arity {
                                return Err(Self::error(
                                    chunk,
                                    ip,
                                    &format!(
                                        "Expected {} arguments but got {}.",
                                        native.arity, argc
                                    ),
                                ));
                            }
                            let args = &self.stack[callee_slot + 1..callee_slot + 1 + argc];
                            let res =
                                (native.func)(args).map_err(|msg| Self::error(chunk, ip, &msg))?;
                            self.stack.truncate(callee_slot);
                            self.stack.push(res);
                        }
                        _ => {
                            return Err(Self::error(
                                chunk,
                                ip,
                                "Can only call functions and classes.",
                            ));
                        }
                    }
                }
                OpCode::PrepareInvoke(k) => {
                    let name = Self::name_at(chunk, k);
                    let receiver = self.peek();
                    match receiver {
                        Value::VmInstance(instance) => {
                            let callable = if let Some(val) = instance.borrow().fields.get(name) {
                                PreparedCallable::Field(val.clone())
                            } else {
                                let method_opt = {
                                    let class = Rc::clone(&instance.borrow().class);
                                    class.borrow().methods.get(name).cloned()
                                };
                                if let Some(method) = method_opt {
                                    PreparedCallable::Method(method)
                                } else {
                                    return Err(Self::error(
                                        chunk,
                                        ip,
                                        &format!("Undefined property '{}'.", name),
                                    ));
                                }
                            };
                            let frame_depth = self.frames.len();
                            let receiver_slot = self.stack.len() - 1;
                            self.prepared_calls.push(PreparedCall {
                                callable,
                                frame_depth,
                                receiver_slot,
                            });
                        }
                        _ => return Err(Self::error(chunk, ip, "Only instances have properties.")),
                    }
                }
                OpCode::PrepareSuperInvoke(k) => {
                    let name = Self::name_at(chunk, k);
                    let superclass_val = self.pop();
                    let superclass = match superclass_val {
                        Value::VmClass(ref k) => Rc::clone(k),
                        _ => unreachable!("OP_PREPARE_SUPER_INVOKE espera una clase"),
                    };

                    if let Some(method) = superclass.borrow().methods.get(name).cloned() {
                        let frame_depth = self.frames.len();
                        let receiver_slot = self.stack.len() - 1;
                        self.prepared_calls.push(PreparedCall {
                            callable: PreparedCallable::Method(method),
                            frame_depth,
                            receiver_slot,
                        });
                    } else {
                        return Err(Self::error(
                            chunk,
                            ip,
                            &format!("Undefined property '{}'.", name),
                        ));
                    }
                }
                OpCode::Invoke(argc) => {
                    let argc = argc as usize;
                    let callee_slot = self.stack.len() - 1 - argc;
                    let prep = self
                        .prepared_calls
                        .pop()
                        .expect("OP_INVOKE sin OP_PREPARE_INVOKE previo");

                    assert_eq!(
                        prep.frame_depth,
                        self.frames.len(),
                        "desincronización de frame_depth en OP_INVOKE"
                    );
                    assert_eq!(
                        prep.receiver_slot, callee_slot,
                        "desincronización de receiver_slot en OP_INVOKE"
                    );

                    self.frames.last_mut().expect("no hay frame").ip = ip;

                    match prep.callable {
                        PreparedCallable::Method(method) => {
                            self.call_closure(Rc::clone(&method), argc)
                                .map_err(|msg| Self::error(chunk, ip, &msg))?;
                            closure = method;
                            ip = 0;
                            base = callee_slot;
                            ic_base = self.frames.last().expect("no hay frame").ic_base;
                        }
                        PreparedCallable::Field(field_val) => {
                            self.stack[callee_slot] = field_val;

                            match &self.stack[callee_slot] {
                                Value::Closure(c) => {
                                    let c = Rc::clone(c);
                                    self.call_closure(Rc::clone(&c), argc)
                                        .map_err(|msg| Self::error(chunk, ip, &msg))?;
                                    closure = c;
                                    ip = 0;
                                    base = callee_slot;
                                    ic_base = self.frames.last().expect("no hay frame").ic_base;
                                }
                                Value::BoundMethod(bound) => {
                                    let receiver = bound.receiver.clone();
                                    let method = Rc::clone(&bound.method);
                                    self.stack[callee_slot] = receiver;
                                    self.call_closure(Rc::clone(&method), argc)
                                        .map_err(|msg| Self::error(chunk, ip, &msg))?;
                                    closure = method;
                                    ip = 0;
                                    base = callee_slot;
                                    ic_base = self.frames.last().expect("no hay frame").ic_base;
                                }
                                Value::VmClass(klass) => {
                                    let klass = Rc::clone(klass);
                                    let instance =
                                        Rc::new(RefCell::new(VmInstance::new(Rc::clone(&klass))));
                                    self.stack[callee_slot] = Value::VmInstance(instance);

                                    let init = klass.borrow().methods.get("init").cloned();
                                    if let Some(init) = init {
                                        self.call_closure(Rc::clone(&init), argc)
                                            .map_err(|msg| Self::error(chunk, ip, &msg))?;
                                        closure = init;
                                        ip = 0;
                                        base = callee_slot;
                                        ic_base = self.frames.last().expect("no hay frame").ic_base;
                                    } else if argc != 0 {
                                        return Err(Self::error(
                                            chunk,
                                            ip,
                                            &format!("Expected 0 arguments but got {}.", argc),
                                        ));
                                    }
                                }
                                _ => {
                                    return Err(Self::error(
                                        chunk,
                                        ip,
                                        "Can only call functions and classes.",
                                    ));
                                }
                            }
                        }
                    }
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
                            let mut inst = instance.borrow_mut();
                            if let Some(field) = inst.fields.get_mut(name) {
                                *field = value.clone();
                            } else {
                                inst.fields.insert(name.to_string(), value.clone());
                            }
                            drop(inst);
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
                    ic_base = caller.ic_base;
                }
                OpCode::BuildList(count) => {
                    let count = count as usize;
                    let start = self.stack.len() - count;
                    let elements: Vec<Value> = self.stack.drain(start..).collect();
                    self.stack
                        .push(Value::List(Rc::new(RefCell::new(elements))));
                }
                OpCode::GetIndex => {
                    let idx = self.pop();
                    let obj = self.pop();
                    match obj {
                        Value::List(l) => {
                            let items = l.borrow();
                            let i = crate::value::validate_list_index(items.len(), &idx)
                                .map_err(|msg| Self::error(chunk, ip, &msg))?;
                            self.stack.push(items[i].clone());
                        }
                        _ => return Err(Self::error(chunk, ip, "Only lists can be indexed.")),
                    }
                }
                OpCode::SetIndex => {
                    let val = self.pop();
                    let idx = self.pop();
                    let obj = self.pop();
                    match obj {
                        Value::List(l) => {
                            let mut items = l.borrow_mut();
                            let i = crate::value::validate_list_index(items.len(), &idx)
                                .map_err(|msg| Self::error(chunk, ip, &msg))?;
                            items[i] = val.clone();
                            drop(items);
                            self.stack.push(val);
                        }
                        _ => return Err(Self::error(chunk, ip, "Only lists can be indexed.")),
                    }
                }
                OpCode::PrepareIndexModify => {
                    let idx = self.stack.last().unwrap().clone();
                    let obj = self.stack[self.stack.len() - 2].clone();
                    match obj {
                        Value::List(l) => {
                            let items = l.borrow();
                            let i = crate::value::validate_list_index(items.len(), &idx)
                                .map_err(|msg| Self::error(chunk, ip, &msg))?;
                            let cur = items[i].clone();
                            drop(items);
                            self.stack.push(cur);
                        }
                        _ => return Err(Self::error(chunk, ip, "Only lists can be indexed.")),
                    }
                }
                OpCode::AddLocalConst(slot, k) => {
                    let a = &self.stack[base + slot as usize];
                    let b = &chunk.constants[k as usize];
                    match (a, b) {
                        (Value::Number(x), Value::Number(y)) => {
                            self.stack.push(Value::Number(x + y));
                        }
                        (Value::String(x), Value::String(y)) => {
                            let mut s = String::with_capacity(x.len() + y.len());
                            s.push_str(x);
                            s.push_str(y);
                            self.stack.push(Value::String(s));
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
                OpCode::LessLocalConst(slot, k) => {
                    let a = &self.stack[base + slot as usize];
                    let b = &chunk.constants[k as usize];
                    match (a, b) {
                        (Value::Number(x), Value::Number(y)) => {
                            self.stack.push(Value::Boolean(x < y));
                        }
                        _ => {
                            return Err(Self::error(chunk, ip, "Operands must be numbers."));
                        }
                    }
                }
            }
        }
    }
}
