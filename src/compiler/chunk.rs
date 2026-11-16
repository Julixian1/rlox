//! Definición de opcodes, fragmentos de bytecode (`Chunk`) y utilidades de desensamblado para la Stack VM.

use crate::value::Value;
use std::fmt;

/// Conjunto de instrucciones ejecutables por la máquina virtual basada en pila (Stack VM).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    /// Carga una constante desde el pool de constantes en el tope de la pila.
    Constant(u16),
    /// Apila el valor `nil`.
    Nil,
    /// Apila el valor booleano `true`.
    True,
    /// Apila el valor booleano `false`.
    False,

    /// Descarta el valor ubicado en el tope de la pila.
    Pop,
    /// Duplica el valor ubicado en el tope de la pila.
    Dup,

    /// Carga el valor de una variable local desde la ranura indicada.
    GetLocal(u16),
    /// Almacena el valor del tope de la pila en la variable local indicada.
    SetLocal(u16),
    /// Almacena y descarta el valor en la variable local indicada.
    SetLocalPop(u16),
    /// Obtiene una variable global por nombre.
    GetGlobal(u16),
    /// Define una nueva variable global por nombre.
    DefineGlobal(u16),
    /// Asigna un valor a una variable global existente.
    SetGlobal(u16),
    /// Lectura rápida de variable global mediante índice resuelto.
    GetGlobalFast(u16),
    /// Escritura rápida de variable global mediante índice resuelto.
    SetGlobalFast(u16),

    /// Compara igualdad entre los dos operandos superiores.
    Equal,
    /// Compara si el penúltimo valor es mayor que el tope.
    Greater,
    /// Compara si el penúltimo valor es menor que el tope.
    Less,
    /// Compara si el penúltimo valor es mayor o igual que el tope.
    GreaterEqual,
    /// Compara si el penúltimo valor es menor o igual que el tope.
    LessEqual,
    /// Suma los dos valores del tope de la pila (o concatena cadenas).
    Add,
    /// Resta el tope de la pila del penúltimo valor.
    Subtract,
    /// Multiplica los dos valores superiores de la pila.
    Multiply,
    /// Divide el penúltimo valor por el tope de la pila.
    Divide,
    /// Calcula el resto de la división entera (módulo).
    Modulo,
    /// Aplica la negación lógica (`!`).
    Not,
    /// Aplica la negación aritmética unaria (`-`).
    Negate,

    /// Imprime en la salida estándar el valor del tope y lo descarta.
    Print,
    /// Salto incondicional relativo hacia adelante.
    Jump(u16),
    /// Salto condicional relativo hacia adelante si el tope es falso.
    JumpIfFalse(u16),
    /// Salto incondicional relativo hacia atrás en un bucle.
    Loop(u16),

    /// Invoca una función o clausura con la cantidad indicada de argumentos.
    Call(u8),
    /// Retorna de la función actual.
    Return,

    /// Crea una clausura a partir del prototipo almacenado en constantes.
    Closure(u16),
    /// Obtiene el valor de una variable capturada (upvalue).
    GetUpvalue(u16),
    /// Modifica el valor de una variable capturada (upvalue).
    SetUpvalue(u16),
    /// Cierra un upvalue abierto moviéndolo al heap.
    CloseUpvalue,

    /// Declara una nueva clase.
    Class(u16),
    /// Asocia un método a la clase actual en la pila.
    Method(u16),
    /// Obtiene una propiedad de la instancia en el tope.
    GetProperty(u16),
    /// Establece el valor de una propiedad en la instancia.
    SetProperty(u16),
    /// Verifica que el operando sea una instancia válida.
    CheckInstance,
    /// Prepara una invocación optimizada de método.
    PrepareInvoke(u16),
    /// Prepara una invocación optimizada de método sobre `super`.
    PrepareSuperInvoke(u16),
    /// Invoca el método previamente preparado con sus argumentos.
    Invoke(u8),

    /// Establece la relación de herencia entre clase base y subclase.
    Inherit,
    /// Busca un método en la superclase indicada.
    GetSuper(u16),

    /// Superinstrucción: suma una constante numérica a una ranura local.
    AddLocalConst(u8, u16),
    /// Superinstrucción: compara si una ranura local es menor que una constante.
    LessLocalConst(u8, u16),

    /// Construye una lista nativa a partir de `N` elementos apilados.
    BuildList(u16),
    /// Obtiene un elemento de una lista por índice.
    GetIndex,
    /// Asigna un elemento en una lista por índice.
    SetIndex,
    /// Prepara los operandos en la pila para la modificación por índice.
    PrepareIndexModify,
}

/// Contenedor de una secuencia lineal de bytecode, tabla de constantes e información de líneas.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chunk {
    /// Vector de instrucciones de bytecode.
    pub code: Vec<OpCode>,
    /// Tabla de valores constantes referenciados por el código.
    pub constants: Vec<Value>,
    /// Número de línea del código fuente original correspondiente a cada instrucción.
    pub lines: Vec<usize>,
}

/// Error emitido cuando un chunk supera el límite máximo de constantes representables (`u16::MAX`).
#[derive(Debug, PartialEq)]
pub struct TooManyConstants;

impl Chunk {
    /// Crea un nuevo fragmento de bytecode vacío.
    pub fn new() -> Self {
        Self::default()
    }

    /// Añade una instrucción al chunk registrando su número de línea y retorna su índice.
    pub fn write(&mut self, op: OpCode, line: usize) -> usize {
        self.code.push(op);
        self.lines.push(line);
        self.code.len() - 1
    }

    /// Agrega un valor a la tabla de constantes y retorna su índice numérico.
    pub fn add_constant(&mut self, value: Value) -> Result<u16, TooManyConstants> {
        let index = self.constants.len();
        if index > u16::MAX as usize {
            return Err(TooManyConstants);
        }
        self.constants.push(value);
        Ok(index as u16)
    }

    /// Emite una instrucción de salto provisional que será parcheada posteriormente.
    pub fn emit_jump(&mut self, op: OpCode, line: usize) -> usize {
        debug_assert!(matches!(op, OpCode::Jump(_) | OpCode::JumpIfFalse(_)));
        self.write(op, line)
    }

    /// Parchea el desplazamiento relativo de una instrucción de salto previamente emitida.
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

    /// Emite una instrucción de salto hacia atrás (`OpCode::Loop`) hacia el inicio del bucle.
    pub fn emit_loop(&mut self, loop_start: usize, line: usize) -> Result<(), ()> {
        // +1 porque `ip` ya avanzó más allá del propio `Loop` al ejecutarlo.
        let offset = self.code.len() - loop_start + 1;
        let offset = u16::try_from(offset).map_err(|_| ())?;
        self.write(OpCode::Loop(offset), line);
        Ok(())
    }

    /// Desensambla el contenido completo del chunk en formato textual legible.
    pub fn disassemble(&self, name: &str) -> String {
        let mut out = format!("== {name} ==\n");
        for offset in 0..self.code.len() {
            out.push_str(&self.disassemble_instruction(offset));
            out.push('\n');
        }
        for constant in &self.constants {
            if let Value::Proto(proto) = constant {
                out.push('\n');
                out.push_str(&proto.chunk.disassemble(&proto.to_string()));
            }
        }
        out
    }

    /// Desensambla una instrucción individual en la posición indicada.
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
            | OpCode::SetGlobal(i)
            | OpCode::GetGlobalFast(i)
            | OpCode::SetGlobalFast(i)
            | OpCode::Class(i)
            | OpCode::Method(i)
            | OpCode::GetProperty(i)
            | OpCode::SetProperty(i)
            | OpCode::GetSuper(i)
            | OpCode::PrepareInvoke(i)
            | OpCode::PrepareSuperInvoke(i) => {
                format!(
                    "{:<14} {:4} '{}'",
                    name_of(&op),
                    i,
                    self.constants[i as usize]
                )
            }
            OpCode::GetLocal(s)
            | OpCode::SetLocal(s)
            | OpCode::SetLocalPop(s)
            | OpCode::GetUpvalue(s)
            | OpCode::SetUpvalue(s) => format!("{:<14} {:4}", name_of(&op), s),
            OpCode::Closure(i) => {
                let value = &self.constants[i as usize];
                let mut text = format!("{:<14} {:4} '{}'", name_of(&op), i, value);
                // Qué captura la función: una línea por upvalue.
                if let Value::Proto(proto) = value {
                    for up in &proto.upvalues {
                        let kind = if up.is_local { "local" } else { "upvalue" };
                        text.push_str(&format!("\n | {kind} {}", up.index));
                    }
                }
                text
            }
            OpCode::Jump(o) | OpCode::JumpIfFalse(o) => {
                format!(
                    "{:<14} {:4} -> {}",
                    name_of(&op),
                    o,
                    offset + 1 + o as usize
                )
            }
            OpCode::Loop(o) => {
                format!(
                    "{:<14} {:4} -> {}",
                    name_of(&op),
                    o,
                    offset + 1 - o as usize
                )
            }
            OpCode::Call(n) | OpCode::Invoke(n) => format!("{:<14} {:4}", name_of(&op), n),
            OpCode::BuildList(n) => format!("{:<14} {:4}", name_of(&op), n),
            OpCode::AddLocalConst(slot, k) | OpCode::LessLocalConst(slot, k) => {
                format!(
                    "{:<14} slot:{} const:{} '{}'",
                    name_of(&op),
                    slot,
                    k,
                    self.constants[k as usize]
                )
            }
            _ => name_of(&op).to_string(),
        };
        format!("{offset:04} {line} {detail}")
    }
}

/// Retorna el nombre mnemónico de una instrucción de bytecode para desensamblado.
fn name_of(op: &OpCode) -> &'static str {
    match op {
        OpCode::Constant(_) => "OP_CONSTANT",
        OpCode::Nil => "OP_NIL",
        OpCode::True => "OP_TRUE",
        OpCode::False => "OP_FALSE",
        OpCode::Pop => "OP_POP",
        OpCode::Dup => "OP_DUP",
        OpCode::GetLocal(_) => "OP_GET_LOCAL",
        OpCode::SetLocal(_) => "OP_SET_LOCAL",
        OpCode::SetLocalPop(_) => "OP_SET_LOCAL_POP",
        OpCode::GetGlobal(_) => "OP_GET_GLOBAL",
        OpCode::DefineGlobal(_) => "OP_DEFINE_GLOBAL",
        OpCode::SetGlobal(_) => "OP_SET_GLOBAL",
        OpCode::GetGlobalFast(_) => "OP_GET_GLOBAL_FAST",
        OpCode::SetGlobalFast(_) => "OP_SET_GLOBAL_FAST",
        OpCode::Equal => "OP_EQUAL",
        OpCode::Greater => "OP_GREATER",
        OpCode::Less => "OP_LESS",
        OpCode::GreaterEqual => "OP_GREATER_EQUAL",
        OpCode::LessEqual => "OP_LESS_EQUAL",
        OpCode::Add => "OP_ADD",
        OpCode::Subtract => "OP_SUBTRACT",
        OpCode::Multiply => "OP_MULTIPLY",
        OpCode::Divide => "OP_DIVIDE",
        OpCode::Modulo => "OP_MODULO",
        OpCode::Not => "OP_NOT",
        OpCode::Negate => "OP_NEGATE",
        OpCode::Print => "OP_PRINT",
        OpCode::Jump(_) => "OP_JUMP",
        OpCode::JumpIfFalse(_) => "OP_JUMP_IF_FALSE",
        OpCode::Loop(_) => "OP_LOOP",
        OpCode::Call(_) => "OP_CALL",
        OpCode::Return => "OP_RETURN",
        OpCode::Closure(_) => "OP_CLOSURE",
        OpCode::GetUpvalue(_) => "OP_GET_UPVALUE",
        OpCode::SetUpvalue(_) => "OP_SET_UPVALUE",
        OpCode::CloseUpvalue => "OP_CLOSE_UPVALUE",
        OpCode::Class(_) => "OP_CLASS",
        OpCode::Method(_) => "OP_METHOD",
        OpCode::GetProperty(_) => "OP_GET_PROPERTY",
        OpCode::SetProperty(_) => "OP_SET_PROPERTY",
        OpCode::CheckInstance => "OP_CHECK_INSTANCE",
        OpCode::Inherit => "OP_INHERIT",
        OpCode::GetSuper(_) => "OP_GET_SUPER",
        OpCode::PrepareInvoke(_) => "OP_PREPARE_INVOKE",
        OpCode::PrepareSuperInvoke(_) => "OP_PREPARE_SUPER_INVOKE",
        OpCode::Invoke(_) => "OP_INVOKE",
        OpCode::AddLocalConst(_, _) => "OP_ADD_LOCAL_CONST",
        OpCode::LessLocalConst(_, _) => "OP_LESS_LOCAL_CONST",
        OpCode::BuildList(_) => "OP_BUILD_LIST",
        OpCode::GetIndex => "OP_GET_INDEX",
        OpCode::SetIndex => "OP_SET_INDEX",
        OpCode::PrepareIndexModify => "OP_PREPARE_INDEX_MODIFY",
    }
}

impl fmt::Display for Chunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.disassemble("chunk"))
    }
}
