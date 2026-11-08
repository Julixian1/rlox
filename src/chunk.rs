// Representación del bytecode: instrucciones, constantes e información de líneas.
 
use crate::value::Value;
use std::fmt;
 
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    // Literales
    Constant(u16), // índice en `Chunk::constants`
    Nil,
    True,
    False,
 
    // Stack
    Pop,
 
    // Variables
    GetLocal(u16), // slot relativo al inicio del frame
    SetLocal(u16),
    GetGlobal(u16), // índice de la constante con el nombre
    DefineGlobal(u16),
    SetGlobal(u16),
 
    // Comparación y aritmética
    Equal,
    Greater,
    Less,
    Add,
    Subtract,
    Multiply,
    Divide,
    Not,
    Negate,
 
    // Sentencias y control de flujo
    Print,
    Jump(u16),        
    JumpIfFalse(u16), 
    Loop(u16),        
 
    // Funciones
    Call(u8), // cantidad de argumentos
    Return,
}
 
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chunk {
    pub code: Vec<OpCode>,
    pub constants: Vec<Value>,
    pub lines: Vec<usize>,
}
 
#[derive(Debug, PartialEq)]
pub struct TooManyConstants;
 
impl Chunk {
    pub fn new() -> Self {
        Self::default()
    }
 
    pub fn write(&mut self, op: OpCode, line: usize) -> usize {
        self.code.push(op);
        self.lines.push(line);
        self.code.len() - 1
    }
 
    pub fn add_constant(&mut self, value: Value) -> Result<u16, TooManyConstants> {
        let index = self.constants.len();
        if index > u16::MAX as usize {
            return Err(TooManyConstants);
        }
        self.constants.push(value);
        Ok(index as u16)
    }
 
    pub fn emit_jump(&mut self, op: OpCode, line: usize) -> usize {
        debug_assert!(matches!(op, OpCode::Jump(_) | OpCode::JumpIfFalse(_)));
        self.write(op, line)
    }
 
    pub fn patch_jump(&mut self, at: usize) -> Result<(), ()> {
        // `ip` ya apunta a `at + 1` cuando se ejecuta el salto.
        let offset = self.code.len() - at - 1;
        let offset = u16::try_from(offset).map_err(|_| ())?;
        match &mut self.code[at] {
            OpCode::Jump(o) | OpCode::JumpIfFalse(o) => *o = offset,
            other => panic!("patch_jump sobre una instrucción que no es salto: {other:?}"),
        }
        Ok(())
    }
 
    /// Emite un `Loop` que vuelve a `loop_start`.
    pub fn emit_loop(&mut self, loop_start: usize, line: usize) -> Result<(), ()> {
        let offset = self.code.len() - loop_start + 1;
        let offset = u16::try_from(offset).map_err(|_| ())?;
        self.write(OpCode::Loop(offset), line);
        Ok(())
    }
 
    pub fn disassemble(&self, name: &str) -> String {
        let mut out = format!("== {name} ==\n");
        for offset in 0..self.code.len() {
            out.push_str(&self.disassemble_instruction(offset));
            out.push('\n');
        }
        out
    }
 
    pub fn disassemble_instruction(&self, offset: usize) -> String {
        let line = if offset > 0 && self.lines[offset] == self.lines[offset - 1] {
            "   |".to_string()
        } else {
            format!("{:4}", self.lines[offset])
        };
        let op = self.code[offset];
        let detail = match op {
            OpCode::Constant(i)
            | OpCode::GetGlobal(i)
            | OpCode::DefineGlobal(i)
            | OpCode::SetGlobal(i) => {
                format!("{:<14} {:4} '{}'", name_of(&op), i, self.constants[i as usize])
            }
            OpCode::GetLocal(s) | OpCode::SetLocal(s) => format!("{:<14} {:4}", name_of(&op), s),
            OpCode::Jump(o) | OpCode::JumpIfFalse(o) => {
                format!("{:<14} {:4} -> {}", name_of(&op), o, offset + 1 + o as usize)
            }
            OpCode::Loop(o) => {
                format!("{:<14} {:4} -> {}", name_of(&op), o, offset + 1 - o as usize)
            }
            OpCode::Call(n) => format!("{:<14} {:4}", name_of(&op), n),
            _ => name_of(&op).to_string(),
        };
        format!("{offset:04} {line} {detail}")
    }
}
 
fn name_of(op: &OpCode) -> &'static str {
    match op {
        OpCode::Constant(_) => "OP_CONSTANT",
        OpCode::Nil => "OP_NIL",
        OpCode::True => "OP_TRUE",
        OpCode::False => "OP_FALSE",
        OpCode::Pop => "OP_POP",
        OpCode::GetLocal(_) => "OP_GET_LOCAL",
        OpCode::SetLocal(_) => "OP_SET_LOCAL",
        OpCode::GetGlobal(_) => "OP_GET_GLOBAL",
        OpCode::DefineGlobal(_) => "OP_DEFINE_GLOBAL",
        OpCode::SetGlobal(_) => "OP_SET_GLOBAL",
        OpCode::Equal => "OP_EQUAL",
        OpCode::Greater => "OP_GREATER",
        OpCode::Less => "OP_LESS",
        OpCode::Add => "OP_ADD",
        OpCode::Subtract => "OP_SUBTRACT",
        OpCode::Multiply => "OP_MULTIPLY",
        OpCode::Divide => "OP_DIVIDE",
        OpCode::Not => "OP_NOT",
        OpCode::Negate => "OP_NEGATE",
        OpCode::Print => "OP_PRINT",
        OpCode::Jump(_) => "OP_JUMP",
        OpCode::JumpIfFalse(_) => "OP_JUMP_IF_FALSE",
        OpCode::Loop(_) => "OP_LOOP",
        OpCode::Call(_) => "OP_CALL",
        OpCode::Return => "OP_RETURN",
    }
}
 
impl fmt::Display for Chunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.disassemble("chunk"))
    }
}
