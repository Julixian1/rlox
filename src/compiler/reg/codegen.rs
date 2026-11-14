use super::chunk::{RegChunk, RegOp};
use crate::expr::{Expr, LiteralValue};
use crate::stmt::Stmt;
use crate::token::{Token, TokenType};
use super::closure::RegProto;
use crate::closure::UpvalueDesc;
use crate::value::Value;
use std::rc::Rc;

type CResult = Result<(), String>;

/// Un local vive en el registro que coincide con su posición en `locals`.
struct Local {
    name: String,
    depth: usize,
    captured: bool,
}

/// Estado de la función que se está compilando.
struct FnState {
    name: Option<String>,
    arity: usize,
    chunk: RegChunk,
    locals: Vec<Local>,
    upvalues: Vec<UpvalueDesc>,
    scope_depth: usize,
    /// Primer registro libre. Los registros `0..locals.len()` son locales.
    free_reg: usize,
}

impl FnState {
    fn new(name: Option<String>, scope_depth: usize) -> Self {
        FnState {
            name,
            arity: 0,
            chunk: RegChunk::new(),
            locals: Vec::new(),
            upvalues: Vec::new(),
            scope_depth,
            free_reg: 0,
        }
    }

    fn add_upvalue(&mut self, desc: UpvalueDesc) -> u16 {
        match self.upvalues.iter().position(|u| *u == desc) {
            Some(i) => i as u16,
            None => {
                self.upvalues.push(desc);
                (self.upvalues.len() - 1) as u16
            }
        }
    }

    fn into_proto(self) -> RegProto {
        RegProto {
            name: self.name,
            arity: self.arity,
            chunk: self.chunk,
            upvalues: self.upvalues,
        }
    }
}

struct RegCompiler {
    st: FnState,
    enclosing: Vec<FnState>,
    line: usize,
}

pub fn compile(statements: &[Stmt]) -> Result<Rc<RegProto>, String> {
    RegCompiler::new().compile(statements)
}

impl RegCompiler {
    fn new() -> Self {
        RegCompiler {
            st: FnState::new(None, 0),
            enclosing: Vec::new(),
            line: 1,
        }
    }

    fn compile(mut self, statements: &[Stmt]) -> Result<Rc<RegProto>, String> {
        for stmt in statements {
            self.stmt(stmt)?;
        }
        self.emit_implicit_return()?;
        Ok(Rc::new(self.st.into_proto()))
    }

    fn emit_implicit_return(&mut self) -> CResult {
        let r = self.alloc_reg()?;
        self.emit(RegOp::LoadNil(r));
        self.emit(RegOp::Return(r));
        Ok(())
    }

    // ---- helpers de emisión --------------------------------------------

    fn emit(&mut self, op: RegOp) -> usize {
        let line = self.line;
        self.st.chunk.write(op, line)
    }

    fn error(&self, msg: &str) -> String {
        format!("[line {}] Error: {}", self.line, msg)
    }

    fn make_constant(&mut self, value: Value) -> Result<u16, String> {
        let result = self.st.chunk.add_constant(value);
        result.map_err(|_| self.error("Too many constants in one chunk."))
    }

    fn identifier_constant(&mut self, name: &str) -> Result<u16, String> {
        let existing = self
            .st.chunk
            .constants
            .iter()
            .position(|c| matches!(c, Value::String(s) if s == name));
        match existing {
            Some(i) => Ok(i as u16),
            None => self.make_constant(Value::String(name.to_string())),
        }
    }

    fn emit_jump(&mut self, op: RegOp) -> usize {
        let line = self.line;
        self.st.chunk.emit_jump(op, line)
    }

    fn patch_jump(&mut self, at: usize) -> CResult {
        let result = self.st.chunk.patch_jump(at);
        result.map_err(|_| self.error("Too much code to jump over."))
    }

    fn emit_loop(&mut self, loop_start: usize) -> CResult {
        let line = self.line;
        let result = self.st.chunk.emit_loop(loop_start, line);
        result.map_err(|_| self.error("Loop body too large."))
    }

    // ---- registros y scopes --------------------------------------------

    fn alloc_reg(&mut self) -> Result<u8, String> {
        if self.st.free_reg > u8::MAX as usize {
            return Err(self.error("Too many registers in one function (max 256)."));
        }
        let r = self.st.free_reg as u8;
        self.st.free_reg += 1;
        self.st.chunk.max_regs = self.st.chunk.max_regs.max(self.st.free_reg);
        Ok(r)
    }

    fn resolve_local(&self, name: &str) -> Option<u8> {
        self.st.locals
            .iter()
            .rposition(|l| l.name == name)
            .map(|i| i as u8)
    }

    /// Busca `name` como variable capturada de una función exterior y devuelve
    /// su índice en la lista de upvalues de la función actual.
    fn resolve_upvalue(&mut self, name: &str) -> Option<u16> {
        let level = self.enclosing.len();
        self.resolve_upvalue_at(level, name)
    }

    /// `level` indexa las funciones: `0..enclosing.len()` son las exteriores y
    /// `enclosing.len()` es la actual.
    fn resolve_upvalue_at(&mut self, level: usize, name: &str) -> Option<u16> {
        if level == 0 {
            return None;
        }
        let parent = level - 1;
        let local = self
            .state_at(parent)
            .locals
            .iter()
            .rposition(|l| l.name == name);
        if let Some(reg) = local {
            self.state_at(parent).locals[reg].captured = true;
            let desc = UpvalueDesc {
                is_local: true,
                index: reg as u16,
            };
            return Some(self.state_at(level).add_upvalue(desc));
        }
        let outer = self.resolve_upvalue_at(parent, name)?;
        let desc = UpvalueDesc {
            is_local: false,
            index: outer,
        };
        Some(self.state_at(level).add_upvalue(desc))
    }

    fn state_at(&mut self, level: usize) -> &mut FnState {
        if level == self.enclosing.len() {
            &mut self.st
        } else {
            &mut self.enclosing[level]
        }
    }

    fn begin_scope(&mut self) {
        self.st.scope_depth += 1;
    }

    fn end_scope(&mut self) {
        self.st.scope_depth -= 1;
        let mut first_captured = None;
        while self
            .st
            .locals
            .last()
            .is_some_and(|l| l.depth > self.st.scope_depth)
        {
            let local = self.st.locals.pop().expect("recién comprobado");
            if local.captured {
                first_captured = Some(self.st.locals.len() as u8);
            }
        }
        if let Some(r) = first_captured {
            self.emit(RegOp::Close(r));
        }
        self.st.free_reg = self.st.locals.len();
    }

    // ---- sentencias ------------------------------------------------------

    fn stmt(&mut self, s: &Stmt) -> CResult {
        debug_assert_eq!(self.st.free_reg, self.st.locals.len());
        match s {
            Stmt::Expression { expression } => self.expr_discard(expression),
            Stmt::Print { expression } => {
                let saved = self.st.free_reg;
                let r = self.operand(expression)?;
                self.emit(RegOp::Print(r));
                self.st.free_reg = saved;
                Ok(())
            }
            Stmt::Var { name, initializer } => self.var_stmt(name, initializer),
            Stmt::Block { statements } => {
                self.begin_scope();
                for st in statements {
                    self.stmt(st)?;
                }
                self.end_scope();
                Ok(())
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let saved = self.st.free_reg;
                let c = self.operand(condition)?;
                let else_jump = self.emit_jump(RegOp::JumpIfFalse(c, 0));
                self.st.free_reg = saved;
                self.stmt(then_branch)?;
                match else_branch {
                    Some(else_stmt) => {
                        let end_jump = self.emit_jump(RegOp::Jump(0));
                        self.patch_jump(else_jump)?;
                        self.stmt(else_stmt)?;
                        self.patch_jump(end_jump)
                    }
                    None => self.patch_jump(else_jump),
                }
            }
            Stmt::While { condition, body } => {
                let loop_start = self.st.chunk.code.len();
                let saved = self.st.free_reg;
                let c = self.operand(condition)?;
                let exit_jump = self.emit_jump(RegOp::JumpIfFalse(c, 0));
                self.st.free_reg = saved;
                self.stmt(body)?;
                self.emit_loop(loop_start)?;
                self.patch_jump(exit_jump)
            }
            Stmt::Function { name, params, body } => self.fun_stmt(name, params, body),
            Stmt::Return { keyword, value } => {
                self.line = keyword.line;
                if self.enclosing.is_empty() {
                    return Err(self.error("Can't return from top-level code."));
                }
                let saved = self.st.free_reg;
                let r = match value {
                    Some(e) => self.operand(e)?,
                    None => {
                        let r = self.alloc_reg()?;
                        self.emit(RegOp::LoadNil(r));
                        r
                    }
                };
                self.line = keyword.line;
                self.emit(RegOp::Return(r));
                self.st.free_reg = saved;
                Ok(())
            }
            Stmt::Class { name, .. } => {
                self.line = name.line;
                Err(self.error("Classes are not supported yet in the register backend."))
            }
        }
    }

    fn fun_stmt(&mut self, name: &Token, params: &[Token], body: &[Stmt]) -> CResult {
        self.line = name.line;
        if self.st.scope_depth == 0 {
            let t = self.alloc_reg()?;
            self.function(name, params, body, t)?;
            let k = self.identifier_constant(&name.lexeme)?;
            self.line = name.line;
            self.emit(RegOp::DefineGlobal(k, t));
            self.st.free_reg = self.st.locals.len();
        } else {
            let dst = self.alloc_reg()?;
            self.st.locals.push(Local {
                name: name.lexeme.clone(),
                depth: self.st.scope_depth,
                captured: false,
            });
            self.function(name, params, body, dst)?;
            self.st.free_reg = self.st.locals.len();
        }
        Ok(())
    }

    /// Compila el cuerpo en un `FnState` nuevo y emite `Closure(dst, k)` en la actual.
    fn function(&mut self, name: &Token, params: &[Token], body: &[Stmt], dst: u8) -> CResult {
        let outer = std::mem::replace(
            &mut self.st,
            FnState::new(Some(name.lexeme.clone()), 1),
        );
        self.enclosing.push(outer);

        let result = self.function_body(params, body);

        let outer = self.enclosing.pop().expect("se apiló arriba");
        let inner = std::mem::replace(&mut self.st, outer);
        result?;

        self.line = name.line;
        let k = self.make_constant(Value::RegProto(Rc::new(inner.into_proto())))?;
        self.emit(RegOp::Closure(dst, k));
        Ok(())
    }

    fn function_body(&mut self, params: &[Token], body: &[Stmt]) -> CResult {
        for param in params {
            self.alloc_reg()?;
            self.st.locals.push(Local {
                name: param.lexeme.clone(),
                depth: 1,
                captured: false,
            });
        }
        self.st.arity = params.len();
        for stmt in body {
            self.stmt(stmt)?;
        }
        self.emit_implicit_return()
    }

    fn var_stmt(&mut self, name: &Token, initializer: &Option<Expr>) -> CResult {
        self.line = name.line;
        if self.st.scope_depth == 0 {
            let t = self.alloc_reg()?;
            self.init_into(initializer, t)?;
            let k = self.identifier_constant(&name.lexeme)?;
            self.emit(RegOp::DefineGlobal(k, t));
            self.st.free_reg = self.st.locals.len();
        } else {
            let dst = self.alloc_reg()?;
            self.init_into(initializer, dst)?;
            self.st.locals.push(Local {
                name: name.lexeme.clone(),
                depth: self.st.scope_depth,
                captured: false,
            });
            self.st.free_reg = self.st.locals.len();
        }
        Ok(())
    }

    fn init_into(&mut self, initializer: &Option<Expr>, dst: u8) -> CResult {
        match initializer {
            Some(e) => self.expr(e, dst),
            None => {
                self.emit(RegOp::LoadNil(dst));
                Ok(())
            }
        }
    }

    fn expr_discard(&mut self, e: &Expr) -> CResult {
        let saved = self.st.free_reg;
        if let Expr::Assign { name, value } = e {
            if let Some(reg) = self.resolve_local(&name.lexeme) {
                self.line = name.line;
                self.expr(value, reg)?;
                self.st.free_reg = saved;
                return Ok(());
            }
        }
        let t = self.alloc_reg()?;
        self.expr(e, t)?;
        self.st.free_reg = saved;
        Ok(())
    }

    // ---- expresiones -------------------------------------------------------

    fn operand(&mut self, e: &Expr) -> Result<u8, String> {
        self.operand_before(e, None)
    }

    fn operand_before(&mut self, e: &Expr, later: Option<&Expr>) -> Result<u8, String> {
        if let Expr::Variable { name } = e {
            if let Some(r) = self.resolve_local(&name.lexeme) {
                if !later.is_some_and(Self::may_assign) {
                    return Ok(r);
                }
            }
        }
        let t = self.alloc_reg()?;
        self.expr(e, t)?;
        Ok(t)
    }

    fn may_assign(e: &Expr) -> bool {
        match e {
            Expr::Assign { .. } | Expr::Call { .. } => true,
            Expr::Unary { right, .. } => Self::may_assign(right),
            Expr::Binary { left, right, .. } | Expr::Logical { left, right, .. } => {
                Self::may_assign(left) || Self::may_assign(right)
            }
            Expr::Grouping { expression } => Self::may_assign(expression),
            Expr::Get { object, .. } => Self::may_assign(object),
            Expr::Set { .. } => true,
            Expr::Literal { .. }
            | Expr::Variable { .. }
            | Expr::This { .. }
            | Expr::Super { .. } => false,
        }
    }

    /// Compila `e` dejando su valor en el registro `dst`.
    fn expr(&mut self, e: &Expr, dst: u8) -> CResult {
        match e {
            Expr::Literal { value } => {
                match value {
                    LiteralValue::Number(n) => {
                        let k = self.make_constant(Value::Number(*n))?;
                        self.emit(RegOp::LoadK(dst, k));
                    }
                    LiteralValue::StringVal(s) => {
                        let k = self.make_constant(Value::String(s.clone()))?;
                        self.emit(RegOp::LoadK(dst, k));
                    }
                    LiteralValue::Bool(b) => {
                        self.emit(RegOp::LoadBool(dst, *b));
                    }
                    LiteralValue::Nil => {
                        self.emit(RegOp::LoadNil(dst));
                    }
                }
                Ok(())
            }

            Expr::Grouping { expression } => self.expr(expression, dst),

            Expr::Variable { name } => {
                self.line = name.line;
                match self.resolve_local(&name.lexeme) {
                    Some(r) => {
                        if r != dst {
                            self.emit(RegOp::Move(dst, r));
                        }
                    }
                    None => match self.resolve_upvalue(&name.lexeme) {
                        Some(i) => {
                            self.emit(RegOp::GetUpvalue(dst, i));
                        }
                        None => {
                            let k = self.identifier_constant(&name.lexeme)?;
                            self.emit(RegOp::GetGlobal(dst, k));
                        }
                    },
                }
                Ok(())
            }

            Expr::Assign { name, value } => {
                match self.resolve_local(&name.lexeme) {
                    Some(r) => {
                        self.expr(value, r)?;
                        if r != dst {
                            self.line = name.line;
                            self.emit(RegOp::Move(dst, r));
                        }
                    }
                    None => {
                        self.expr(value, dst)?;
                        self.line = name.line;
                        match self.resolve_upvalue(&name.lexeme) {
                            Some(i) => {
                                self.emit(RegOp::SetUpvalue(i, dst));
                            }
                            None => {
                                let k = self.identifier_constant(&name.lexeme)?;
                                self.emit(RegOp::SetGlobal(k, dst));
                            }
                        }
                    }
                }
                Ok(())
            }

            Expr::Unary { operator, right } => {
                let saved = self.st.free_reg;
                let s = self.operand(right)?;
                self.line = operator.line;
                let op = match operator.token_type {
                    TokenType::Minus => RegOp::Neg(dst, s),
                    TokenType::Bang => RegOp::Not(dst, s),
                    _ => {
                        return Err(self.error(&format!(
                            "Unexpected unary operator '{}'.",
                            operator.lexeme
                        )));
                    }
                };
                self.emit(op);
                self.st.free_reg = saved;
                Ok(())
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                let saved = self.st.free_reg;
                let l = self.operand_before(left, Some(&**right))?;
                let r = self.operand(right)?;
                self.line = operator.line;
                self.binary_op(operator, dst, l, r)?;
                self.st.free_reg = saved;
                Ok(())
            }

            Expr::Logical {
                left,
                operator,
                right,
            } => {
                let saved = self.st.free_reg;
                let work = if (dst as usize) < self.st.locals.len() {
                    self.alloc_reg()?
                } else {
                    dst
                };
                self.expr(left, work)?;
                self.line = operator.line;
                match operator.token_type {
                    TokenType::And => {
                        let end = self.emit_jump(RegOp::JumpIfFalse(work, 0));
                        self.expr(right, work)?;
                        self.patch_jump(end)?;
                    }
                    _ => {
                        let else_jump = self.emit_jump(RegOp::JumpIfFalse(work, 0));
                        let end_jump = self.emit_jump(RegOp::Jump(0));
                        self.patch_jump(else_jump)?;
                        self.expr(right, work)?;
                        self.patch_jump(end_jump)?;
                    }
                }
                if work != dst {
                    self.emit(RegOp::Move(dst, work));
                }
                self.st.free_reg = saved;
                Ok(())
            }

            Expr::Call {
                callee,
                paren,
                arguments,
            } => {
                // Convención: callee en R[a], argumentos en R[a+1..].
                let saved = self.st.free_reg;
                let a = self.alloc_reg()?;
                self.expr(callee, a)?;
                for arg in arguments {
                    let r = self.alloc_reg()?;
                    self.expr(arg, r)?;
                }
                self.line = paren.line;
                self.emit(RegOp::Call(a, arguments.len() as u8));
                if a != dst {
                    self.emit(RegOp::Move(dst, a));
                }
                self.st.free_reg = saved;
                Ok(())
            }

            Expr::Get { name, .. } | Expr::Set { name, .. } => {
                self.line = name.line;
                Err(self.error("Properties are not supported yet in the register backend."))
            }

            Expr::This { keyword } | Expr::Super { keyword, .. } => {
                self.line = keyword.line;
                Err(self.error("'this' and 'super' are not supported yet in the register backend."))
            }
        }
    }

    fn binary_op(&mut self, operator: &Token, d: u8, l: u8, r: u8) -> CResult {
        match operator.token_type {
            TokenType::Plus => {
                self.emit(RegOp::Add(d, l, r));
            }
            TokenType::Minus => {
                self.emit(RegOp::Sub(d, l, r));
            }
            TokenType::Star => {
                self.emit(RegOp::Mul(d, l, r));
            }
            TokenType::Slash => {
                self.emit(RegOp::Div(d, l, r));
            }
            TokenType::Percent => {
                self.emit(RegOp::Mod(d, l, r));
            }
            TokenType::EqualEqual => {
                self.emit(RegOp::Eq(d, l, r));
            }
            TokenType::Less => {
                self.emit(RegOp::Lt(d, l, r));
            }
            TokenType::Greater => {
                self.emit(RegOp::Lt(d, r, l));
            }
            TokenType::BangEqual => {
                self.emit(RegOp::Eq(d, l, r));
                self.emit(RegOp::Not(d, d));
            }
            TokenType::GreaterEqual => {
                self.emit(RegOp::Lt(d, l, r));
                self.emit(RegOp::Not(d, d));
            }
            TokenType::LessEqual => {
                self.emit(RegOp::Lt(d, r, l));
                self.emit(RegOp::Not(d, d));
            }
            _ => {
                return Err(self.error(&format!(
                    "Unexpected binary operator '{}'.",
                    operator.lexeme
                )));
            }
        }
        Ok(())
    }
}
