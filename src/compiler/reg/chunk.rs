//! Instrucciones de bytecode basado en registros (`RegOp`) y estructura del chunk (`RegChunk`).

use crate::value::Value;

/// Error emitido cuando un operando excede el tamaño máximo representable (constantes o saltos).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge;

/// Instrucciones de la máquina virtual basada en registros.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegOp {
    /// Carga una constante desde el pool en el registro de destino.
    LoadK(u8, u16),
    /// Carga el valor `nil` en el registro de destino.
    LoadNil(u8),
    /// Carga un valor booleano en el registro de destino.
    LoadBool(u8, bool),
    /// Copia el valor de un registro origen a un registro destino.
    Move(u8, u8),

    /// Suma los valores de dos registros y almacena el resultado en el registro destino.
    Add(u8, u8, u8),
    /// Resta los valores de dos registros y almacena el resultado en el registro destino.
    Sub(u8, u8, u8),
    /// Multiplica los valores de dos registros y almacena el resultado en el registro destino.
    Mul(u8, u8, u8),
    /// Divide los valores de dos registros y almacena el resultado en el registro destino.
    Div(u8, u8, u8),
    /// Calcula el módulo entre dos registros y almacena el resultado en el registro destino.
    Mod(u8, u8, u8),

    /// Aplica la negación aritmética sobre un registro y almacena en el destino.
    Neg(u8, u8),
    /// Aplica la negación lógica sobre un registro y almacena en el destino.
    Not(u8, u8),

    /// Compara igualdad entre dos registros dejando un booleano en el destino.
    Eq(u8, u8, u8),
    /// Compara si el primer registro es menor que el segundo dejando un booleano en el destino.
    Lt(u8, u8, u8),
    /// Compara si el primer registro es menor o igual que el segundo dejando un booleano en el destino.
    Le(u8, u8, u8),

    /// Salto incondicional relativo hacia adelante.
    Jump(u16),
    /// Salto condicional relativo hacia adelante si el registro evaluado es falso.
    JumpIfFalse(u8, u16),
    /// Salto incondicional relativo hacia atrás en un bucle.
    Loop(u16),

    /// Lee una variable global desde la tabla de constantes y la carga en el registro destino.
    GetGlobal(u8, u16),
    /// Asigna el valor de un registro a una variable global existente.
    SetGlobal(u16, u8),
    /// Define una nueva variable global a partir del valor de un registro.
    DefineGlobal(u16, u8),

    /// Llama a la función del registro base con `n` argumentos contiguos (`base+1..=base+n`).
    Call(u8, u8),
    /// Retorna de la función actual devolviendo el valor del registro indicado.
    Return(u8),

    /// Instancia una clausura a partir del prototipo en constantes y la asigna al registro destino.
    Closure(u8, u16),
    /// Obtiene el valor de una variable capturada (upvalue) y lo almacena en el registro destino.
    GetUpvalue(u8, u16),
    /// Modifica el valor de una variable capturada (upvalue) con el contenido del registro origen.
    SetUpvalue(u16, u8),
    /// Cierra todos los upvalues abiertos en el registro indicado o superiores.
    Close(u8),

    /// Construye una lista a partir de una secuencia contigua de registros y la asigna a destino.
    BuildList(u8, u8, u16),
    /// Indexa una lista almacenada en un registro mediante el registro de índice y guarda en destino.
    GetIndex(u8, u8, u8),
    /// Asigna un elemento en una lista indexada utilizando tres registros (lista, índice, valor).
    SetIndex(u8, u8, u8),

    /// Imprime en la salida estándar el valor almacenado en el registro indicado.
    Print(u8),
}

/// Fragmento de código compilado para la Register VM.
#[derive(Default)]
pub struct RegChunk {
    /// Vector de instrucciones de bytecode basadas en registros.
    pub code: Vec<RegOp>,
    /// Número de línea del código fuente correspondiente a cada instrucción.
    pub lines: Vec<usize>,
    /// Tabla de constantes asociadas a este fragmento.
    pub constants: Vec<Value>,
    /// Cantidad de registros que necesita la función (para reservar el stack).
    pub max_regs: usize,
}

impl RegChunk {
    /// Crea un nuevo fragmento de código vacío para la Register VM.
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

    /// Formatea una constante de la tabla según su índice.
    fn constant(&self, k: u16) -> String {
        format!("{}", self.constants[k as usize])
    }

    /// Genera el nemónico y texto descriptivo de los operandos de una instrucción.
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

            BuildList(d, start, n) => ("BuildList", format!("r{d} r{start}.. {n} el(s)")),
            GetIndex(d, l, i) => ("GetIndex", format!("r{d} r{l}[r{i}]")),
            SetIndex(l, i, s) => ("SetIndex", format!("r{l}[r{i}] = r{s}")),

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
