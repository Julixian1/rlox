use crate::value::Value;

/// Error cuando algo no entra en el operando (demasiadas constantes, salto muy largo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge;

/// Instrucciones de la VM de registros.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegOp {
    // Cargas
    LoadK(u8, u16),    // dst, const
    LoadNil(u8),       // dst
    LoadBool(u8, bool), // dst, valor
    Move(u8, u8),      // dst, src

    // Aritmética: dst, izquierda, derecha
    Add(u8, u8, u8),
    Sub(u8, u8, u8),
    Mul(u8, u8, u8),
    Div(u8, u8, u8),
    Mod(u8, u8, u8),

    // Unarias: dst, src
    Neg(u8, u8),
    Not(u8, u8),

    // Comparaciones: dejan un booleano en dst
    Eq(u8, u8, u8),
    Lt(u8, u8, u8),
    Le(u8, u8, u8),

    // Saltos 
    Jump(u16),
    JumpIfFalse(u8, u16), 
    Loop(u16),

    // Globales
    GetGlobal(u8, u16),    
    SetGlobal(u16, u8),   
    DefineGlobal(u16, u8), 

    // Funciones
    /// Llama a la función del registro `a` con `n` argumentos, que están en
    /// `a+1..=a+n`. El resultado queda en `a`.
    Call(u8, u8), // a, n
    Return(u8),

    // Closures
    Closure(u8, u16),    // dst, const (RegProto)
    GetUpvalue(u8, u16), // dst, índice de upvalue
    SetUpvalue(u16, u8), // índice de upvalue, src
    /// Cierra los upvalues abiertos sobre el registro dado o los de arriba.
    Close(u8),

    // Otros
    Print(u8),
}

#[derive(Default)]
pub struct RegChunk {
    pub code: Vec<RegOp>,
    pub lines: Vec<usize>,
    pub constants: Vec<Value>,
    /// Cantidad de registros que necesita la función (para reservar el stack).
    pub max_regs: usize,
}

impl RegChunk {
    pub fn new() -> Self {
        Self::default()
    }

    /// Agrega una instrucción y devuelve su índice.
    pub fn write(&mut self, op: RegOp, line: usize) -> usize {
        self.code.push(op);
        self.lines.push(line);
        self.code.len() - 1
    }

    /// Agrega una constante y devuelve su índice.
    pub fn add_constant(&mut self, value: Value) -> Result<u16, TooLarge> {
        self.constants.push(value);
        u16::try_from(self.constants.len() - 1).map_err(|_| TooLarge)
    }

    /// Emite un salto con offset 0 (placeholder) para parchear después.
    pub fn emit_jump(&mut self, op: RegOp, line: usize) -> usize {
        self.write(op, line)
    }

    /// Parchea el salto en `at` para que apunte a la próxima instrucción a emitir.
    pub fn patch_jump(&mut self, at: usize) -> Result<(), TooLarge> {
        let offset = u16::try_from(self.code.len() - at - 1).map_err(|_| TooLarge)?;
        match &mut self.code[at] {
            RegOp::Jump(o) | RegOp::JumpIfFalse(_, o) => *o = offset,
            other => unreachable!("patch_jump sobre una instrucción que no salta: {other:?}"),
        }
        Ok(())
    }

    /// Emite un `Loop` que vuelve a `loop_start`.
    pub fn emit_loop(&mut self, loop_start: usize, line: usize) -> Result<(), TooLarge> {
        let offset = u16::try_from(self.code.len() + 1 - loop_start).map_err(|_| TooLarge)?;
        self.write(RegOp::Loop(offset), line);
        Ok(())
    }

    // ---- disassembler ---------------------------------------------------

    /// Devuelve el bytecode legible, una instrucción por línea.
    pub fn disassemble(&self, name: &str) -> String {
        let mut out = format!("== {name} ==\n");
        for (i, op) in self.code.iter().enumerate() {
            let line = if i > 0 && self.lines[i] == self.lines[i - 1] {
                "   |".to_string()
            } else {
                format!("{:>4}", self.lines[i])
            };
            let (mnemonic, operands) = self.describe(i, *op);
            out.push_str(&format!("{i:04} {line}  {mnemonic:<13}{operands}\n"));
        }
        // Las funciones están en el pool de constantes: se desensamblan también.
        for constant in &self.constants {
            if let Value::RegProto(proto) = constant {
                out.push('\n');
                out.push_str(&proto.chunk.disassemble(&proto.to_string()));
            }
        }
        out
    }

    fn constant(&self, k: u16) -> String {
        format!("{}", self.constants[k as usize])
    }

    fn describe(&self, at: usize, op: RegOp) -> (&'static str, String) {
        use RegOp::*;
        match op {
            LoadK(d, k) => ("LoadK", format!("r{d} k{k} ; {}", self.constant(k))),
            LoadNil(d) => ("LoadNil", format!("r{d}")),
            LoadBool(d, b) => ("LoadBool", format!("r{d} {b}")),
            Move(d, s) => ("Move", format!("r{d} r{s}")),

            Add(d, a, b) => ("Add", format!("r{d} r{a} r{b}")),
            Sub(d, a, b) => ("Sub", format!("r{d} r{a} r{b}")),
            Mul(d, a, b) => ("Mul", format!("r{d} r{a} r{b}")),
            Div(d, a, b) => ("Div", format!("r{d} r{a} r{b}")),
            Mod(d, a, b) => ("Mod", format!("r{d} r{a} r{b}")),

            Neg(d, s) => ("Neg", format!("r{d} r{s}")),
            Not(d, s) => ("Not", format!("r{d} r{s}")),

            Eq(d, a, b) => ("Eq", format!("r{d} r{a} r{b}")),
            Lt(d, a, b) => ("Lt", format!("r{d} r{a} r{b}")),
            Le(d, a, b) => ("Le", format!("r{d} r{a} r{b}")),

            Jump(o) => ("Jump", format!("-> {:04}", at + 1 + o as usize)),
            JumpIfFalse(r, o) => ("JumpIfFalse", format!("r{r} -> {:04}", at + 1 + o as usize)),
            Loop(o) => ("Loop", format!("-> {:04}", at + 1 - o as usize)),

            GetGlobal(d, k) => ("GetGlobal", format!("r{d} k{k} ; {}", self.constant(k))),
            SetGlobal(k, s) => ("SetGlobal", format!("k{k} r{s} ; {}", self.constant(k))),
            DefineGlobal(k, s) => ("DefineGlobal", format!("k{k} r{s} ; {}", self.constant(k))),

            Print(r) => ("Print", format!("r{r}")),
            Return(r) => ("Return", format!("r{r}")),

            Call(a, n) => ("Call", format!("r{a} {n} arg(s)")),
            Close(r) => ("Close", format!("r{r}")),
            GetUpvalue(d, i) => ("GetUpvalue", format!("r{d} up{i}")),
            SetUpvalue(i, s) => ("SetUpvalue", format!("up{i} r{s}")),
            Closure(d, k) => {
                let mut text = format!("r{d} k{k} ; {}", self.constant(k));
                // Qué captura la función: una línea por upvalue.
                if let Value::RegProto(proto) = &self.constants[k as usize] {
                    for up in &proto.upvalues {
                        let kind = if up.is_local { "register" } else { "upvalue" };
                        text.push_str(&format!("\n     |                 {kind} {}", up.index));
                    }
                }
                ("Closure", text)
            }
        }
    }
}
