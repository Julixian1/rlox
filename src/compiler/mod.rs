pub mod chunk;
pub mod class;
pub mod closure;
pub mod function;
pub mod opt;
pub mod reg;
pub mod value;
pub mod vm;
pub mod vm_class;

use crate::chunk::{Chunk, OpCode};
use crate::closure::{FnProto, UpvalueDesc};
use crate::expr::{Expr, ExprVisitor, LiteralValue};
use crate::stmt::{Stmt, StmtVisitor};
use crate::token::{Token, TokenType};
use crate::value::Value;
use std::collections::HashSet;
use std::rc::Rc;

pub use opt::{OptConfig, OptLevel, OptPass};

type CResult = Result<(), String>;

#[derive(Clone)]
struct Local {
    name: String,
    depth: usize,
    is_captured: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FnKind {
    Script,
    Function,
    Method,
    Initializer,
}

#[derive(Clone)]
struct LoopContext {
    scope_depth: usize,
    break_jumps: Vec<usize>,
    continue_jumps: Vec<usize>,
}

#[derive(Clone)]
struct FnState {
    proto: FnProto,
    locals: Vec<Local>,
    scope_depth: usize,
    kind: FnKind,
    loop_contexts: Vec<LoopContext>,
}

impl FnState {
    fn new(name: Option<String>, kind: FnKind) -> Self {
        let slot0_name = match kind {
            FnKind::Method | FnKind::Initializer => "this".to_string(),
            _ => String::new(),
        };
        FnState {
            proto: FnProto::new(name),
            locals: vec![Local {
                name: slot0_name,
                depth: 0,
                is_captured: false,
            }],
            scope_depth: 0,
            kind,
            loop_contexts: Vec::new(),
        }
    }

    fn resolve_local(&self, name: &str) -> Option<usize> {
        self.locals.iter().rposition(|l| l.name == name)
    }
}

enum Var {
    Local(u16),
    Upvalue(u16),
    Global,
}

#[derive(Clone)]
struct ClassState {
    has_superclass: bool,
}

#[derive(Clone)]
pub struct Compiler {
    fns: Vec<FnState>,
    classes: Vec<ClassState>,
    line: usize,
    pub opt: OptConfig,
}

pub fn compile(statements: &[Stmt]) -> Result<Rc<FnProto>, String> {
    Compiler::new().compile(statements)
}

pub fn compile_with_opt(statements: &[Stmt], opt: OptConfig) -> Result<Rc<FnProto>, String> {
    Compiler::with_opt(opt).compile(statements)
}

impl Compiler {
    pub fn new() -> Self {
        Self::with_opt(OptConfig::default())
    }

    pub fn with_opt(opt: OptConfig) -> Self {
        Compiler {
            fns: vec![FnState::new(None, FnKind::Script)],
            classes: Vec::new(),
            line: 1,
            opt,
        }
    }

    pub fn compile(mut self, statements: &[Stmt]) -> Result<Rc<FnProto>, String> {
        self.compile_stmt_sequence(statements)?;
        self.emit(OpCode::Nil);
        self.emit(OpCode::Return);
        let mut script = self.fns.pop().expect("falta el estado del script");
        if self.opt.jump_simplification {
            simplify_chunk_jumps(&mut script.proto.chunk);
        }
        Ok(Rc::new(script.proto))
    }

    // ---- helpers de emisión --------------------------------------------

    fn state(&mut self) -> &mut FnState {
        self.fns.last_mut().expect("pila de funciones vacía")
    }

    fn chunk(&mut self) -> &mut Chunk {
        &mut self.state().proto.chunk
    }

    fn emit(&mut self, op: OpCode) -> usize {
        let line = self.line;
        self.chunk().write(op, line)
    }

    fn error(&self, msg: &str) -> String {
        format!("[line {}] Error: {}", self.line, msg)
    }

    fn make_constant(&mut self, value: Value) -> Result<u16, String> {
        let result = self.chunk().add_constant(value);
        result.map_err(|_| self.error("Too many constants in one chunk."))
    }

    fn identifier_constant(&mut self, name: &str) -> Result<u16, String> {
        let existing = self
            .chunk()
            .constants
            .iter()
            .position(|c| matches!(c, Value::String(s) if s == name));
        match existing {
            Some(i) => Ok(i as u16),
            None => self.make_constant(Value::String(name.to_string())),
        }
    }

    fn emit_jump(&mut self, op: OpCode) -> usize {
        let line = self.line;
        self.chunk().emit_jump(op, line)
    }

    fn patch_jump(&mut self, at: usize) -> CResult {
        let result = self.chunk().patch_jump(at);
        result.map_err(|_| self.error("Too much code to jump over."))
    }

    fn emit_loop(&mut self, loop_start: usize) -> CResult {
        let line = self.line;
        let result = self.chunk().emit_loop(loop_start, line);
        result.map_err(|_| self.error("Loop body too large."))
    }

    // ---- scopes y locales ----------------------------------------------

    fn begin_scope(&mut self) {
        self.state().scope_depth += 1;
    }

    fn end_scope(&mut self) {
        self.state().scope_depth -= 1;
        loop {
            let state = self.state();
            let captured = match state.locals.last() {
                Some(l) if l.depth > state.scope_depth => l.is_captured,
                _ => break,
            };
            state.locals.pop();
            self.emit(if captured {
                OpCode::CloseUpvalue
            } else {
                OpCode::Pop
            });
        }
    }

    fn add_local_name(&mut self, name: &str) -> CResult {
        if self.state().locals.len() > u16::MAX as usize {
            return Err(self.error("Too many local variables."));
        }
        let depth = self.state().scope_depth;
        self.state().locals.push(Local {
            name: name.to_string(),
            depth,
            is_captured: false,
        });
        Ok(())
    }

    fn add_local(&mut self, name: &Token) -> CResult {
        self.add_local_name(&name.lexeme)
    }

    fn emit_get_var(&mut self, var: &Var, name: &str) -> CResult {
        match var {
            Var::Local(slot) => {
                self.emit(OpCode::GetLocal(*slot));
            }
            Var::Upvalue(index) => {
                self.emit(OpCode::GetUpvalue(*index));
            }
            Var::Global => {
                let k = self.identifier_constant(name)?;
                if self.opt.indexed_globals {
                    self.emit(OpCode::GetGlobalFast(k));
                } else {
                    self.emit(OpCode::GetGlobal(k));
                }
            }
        }
        Ok(())
    }

    fn emit_compound_op(&mut self, operator: &Token) -> CResult {
        match operator.token_type {
            TokenType::PlusEqual => {
                self.emit(OpCode::Add);
            }
            TokenType::MinusEqual => {
                self.emit(OpCode::Subtract);
            }
            TokenType::StarEqual => {
                self.emit(OpCode::Multiply);
            }
            TokenType::SlashEqual => {
                self.emit(OpCode::Divide);
            }
            TokenType::PercentEqual => {
                self.emit(OpCode::Modulo);
            }
            _ => return Err(self.error("Invalid compound operator.")),
        }
        Ok(())
    }

    // ---- resolución de variables ---------------------------------------

    fn resolve_variable(&mut self, name: &str) -> Result<Var, String> {
        let current = self.fns.len() - 1;
        if let Some(slot) = self.fns[current].resolve_local(name) {
            return Ok(Var::Local(slot as u16));
        }
        if let Some(index) = self.resolve_upvalue(current, name)? {
            return Ok(Var::Upvalue(index));
        }
        Ok(Var::Global)
    }

    fn resolve_upvalue(&mut self, level: usize, name: &str) -> Result<Option<u16>, String> {
        if level == 0 {
            return Ok(None);
        }
        let enclosing = level - 1;

        let desc = if let Some(slot) = self.fns[enclosing].resolve_local(name) {
            self.fns[enclosing].locals[slot].is_captured = true;
            UpvalueDesc {
                is_local: true,
                index: slot as u16,
            }
        } else if let Some(index) = self.resolve_upvalue(enclosing, name)? {
            UpvalueDesc {
                is_local: false,
                index,
            }
        } else {
            return Ok(None);
        };

        let index = self.fns[level].proto.add_upvalue(desc);
        if index > u16::MAX as usize {
            return Err(self.error("Too many closure variables in function."));
        }
        Ok(Some(index as u16))
    }

    fn compile_function_body(
        &mut self,
        name: &Token,
        params: &[Token],
        body: &[Stmt],
        kind: FnKind,
    ) -> CResult {
        if params.len() > u8::MAX as usize {
            return Err(self.error("Can't have more than 255 parameters."));
        }

        let mut state = FnState::new(Some(name.lexeme.clone()), kind);
        state.scope_depth = 1;
        state.proto.arity = params.len();
        for param in params {
            state.locals.push(Local {
                name: param.lexeme.clone(),
                depth: 1,
                is_captured: false,
            });
        }
        self.fns.push(state);

        self.compile_stmt_sequence(body)?;

        if kind == FnKind::Initializer {
            self.emit(OpCode::GetLocal(0));
            self.emit(OpCode::Return);
        } else {
            self.emit(OpCode::Nil);
            self.emit(OpCode::Return);
        }

        let mut state = self.fns.pop().expect("falta el estado de la función");
        self.line = name.line;
        if self.opt.jump_simplification {
            simplify_chunk_jumps(&mut state.proto.chunk);
        }
        let k = self.make_constant(Value::Proto(Rc::new(state.proto)))?;
        self.emit(OpCode::Closure(k));
        Ok(())
    }

    // ---- optimizaciones: constant folding ------------------------------

    fn emit_constant_val(&mut self, val: Value) -> CResult {
        match val {
            Value::Boolean(true) => {
                self.emit(OpCode::True);
            }
            Value::Boolean(false) => {
                self.emit(OpCode::False);
            }
            Value::Nil => {
                self.emit(OpCode::Nil);
            }
            _ => {
                let k = self.make_constant(val)?;
                self.emit(OpCode::Constant(k));
            }
        }
        Ok(())
    }

    fn try_eval_const(&self, expr: &Expr) -> Option<Value> {
        match expr {
            Expr::Literal { value } => match value {
                LiteralValue::Number(n) => Some(Value::Number(*n)),
                LiteralValue::Bool(b) => Some(Value::Boolean(*b)),
                LiteralValue::Nil => Some(Value::Nil),
                LiteralValue::StringVal(s) => Some(Value::String(s.clone())),
            },
            Expr::Grouping { expression } => self.try_eval_const(expression),
            Expr::Unary { operator, right } => self.try_eval_unary(&operator.token_type, right),
            Expr::Binary {
                left,
                operator,
                right,
            } => self.try_eval_binary(left, &operator.token_type, right),
            Expr::Ternary {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_val = self.try_eval_const(condition)?;
                if cond_val.is_truthy() {
                    self.try_eval_const(then_branch)
                } else {
                    self.try_eval_const(else_branch)
                }
            }
            _ => None,
        }
    }

    fn try_eval_unary(&self, op: &TokenType, right: &Expr) -> Option<Value> {
        let val = self.try_eval_const(right)?;
        match op {
            TokenType::Minus => match val {
                Value::Number(n) => Some(Value::Number(-n)),
                _ => None,
            },
            _ => None,
        }
    }

    fn try_eval_binary(&self, left: &Expr, op: &TokenType, right: &Expr) -> Option<Value> {
        let l_val = self.try_eval_const(left)?;
        let r_val = self.try_eval_const(right)?;

        match op {
            TokenType::Plus => match (l_val, r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Number(a + b)),
                _ => None,
            },
            TokenType::Minus => match (l_val, r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Number(a - b)),
                _ => None,
            },
            TokenType::Star => match (l_val, r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Number(a * b)),
                _ => None,
            },
            TokenType::Slash => match (l_val, r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Number(a / b)),
                _ => None,
            },
            TokenType::Percent => match (l_val, r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Number(a % b)),
                _ => None,
            },
            TokenType::Less => match (l_val, r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Boolean(a < b)),
                _ => None,
            },
            TokenType::LessEqual => match (l_val, r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Boolean(a <= b)),
                _ => None,
            },
            TokenType::Greater => match (l_val, r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Boolean(a > b)),
                _ => None,
            },
            TokenType::GreaterEqual => match (l_val, r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Boolean(a >= b)),
                _ => None,
            },
            TokenType::EqualEqual => match (&l_val, &r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Boolean(a == b)),
                (Value::String(a), Value::String(b)) => Some(Value::Boolean(a == b)),
                (Value::Boolean(a), Value::Boolean(b)) => Some(Value::Boolean(a == b)),
                (Value::Nil, Value::Nil) => Some(Value::Boolean(true)),
                _ if is_safe_literal(&l_val) && is_safe_literal(&r_val) => {
                    Some(Value::Boolean(false))
                }
                _ => None,
            },
            TokenType::BangEqual => match (&l_val, &r_val) {
                (Value::Number(a), Value::Number(b)) => Some(Value::Boolean(a != b)),
                (Value::String(a), Value::String(b)) => Some(Value::Boolean(a != b)),
                (Value::Boolean(a), Value::Boolean(b)) => Some(Value::Boolean(a != b)),
                (Value::Nil, Value::Nil) => Some(Value::Boolean(false)),
                _ if is_safe_literal(&l_val) && is_safe_literal(&r_val) => {
                    Some(Value::Boolean(true))
                }
                _ => None,
            },
            _ => None,
        }
    }

    // ---- optimizaciones: dead code elimination -------------------------

    fn try_eval_condition(&self, expr: &Expr) -> Option<Value> {
        if self.opt.constant_folding {
            self.try_eval_const(expr)
        } else {
            match expr {
                Expr::Literal { value } => match value {
                    LiteralValue::Bool(b) => Some(Value::Boolean(*b)),
                    LiteralValue::Nil => Some(Value::Nil),
                    LiteralValue::Number(n) => Some(Value::Number(*n)),
                    LiteralValue::StringVal(s) => Some(Value::String(s.clone())),
                },
                Expr::Grouping { expression } => self.try_eval_condition(expression),
                _ => None,
            }
        }
    }

    fn validate_dead_stmt(&self, stmt: &Stmt) -> CResult {
        let mut validator = self.clone();
        validator.opt.dead_code_elimination = false;
        stmt.accept(&mut validator)
    }

    fn validate_dead_stmts(&self, stmts: &[Stmt]) -> CResult {
        let mut validator = self.clone();
        validator.opt.dead_code_elimination = false;
        for stmt in stmts {
            stmt.accept(&mut validator)?;
        }
        Ok(())
    }

    fn compile_stmt_sequence(&mut self, statements: &[Stmt]) -> CResult {
        for (i, stmt) in statements.iter().enumerate() {
            stmt.accept(self)?;
            if matches!(
                stmt,
                Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. }
            ) {
                if self.opt.dead_code_elimination {
                    let dead_tail = &statements[i + 1..];
                    if !dead_tail.is_empty() {
                        self.validate_dead_stmts(dead_tail)?;
                    }
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}

fn is_safe_literal(val: &Value) -> bool {
    matches!(
        val,
        Value::Number(_) | Value::String(_) | Value::Boolean(_) | Value::Nil
    )
}

fn extract_var_name(expr: &Expr) -> Option<&Token> {
    match expr {
        Expr::Variable { name } => Some(name),
        Expr::Grouping { expression } => extract_var_name(expression),
        _ => None,
    }
}

fn extract_literal(expr: &Expr) -> Option<Value> {
    match expr {
        Expr::Literal { value } => match value {
            LiteralValue::Number(n) => Some(Value::Number(*n)),
            LiteralValue::StringVal(s) => Some(Value::String(s.clone())),
            LiteralValue::Bool(b) => Some(Value::Boolean(*b)),
            LiteralValue::Nil => Some(Value::Nil),
        },
        Expr::Grouping { expression } => extract_literal(expression),
        _ => None,
    }
}

fn unwrap_grouping(expr: &Expr) -> &Expr {
    match expr {
        Expr::Grouping { expression } => unwrap_grouping(expression),
        _ => expr,
    }
}

impl ExprVisitor<CResult> for Compiler {
    fn visit_literal_expr(&mut self, value: &LiteralValue) -> CResult {
        match value {
            LiteralValue::Number(n) => {
                let k = self.make_constant(Value::Number(*n))?;
                self.emit(OpCode::Constant(k));
            }
            LiteralValue::StringVal(s) => {
                let k = self.make_constant(Value::String(s.clone()))?;
                self.emit(OpCode::Constant(k));
            }
            LiteralValue::Bool(true) => {
                self.emit(OpCode::True);
            }
            LiteralValue::Bool(false) => {
                self.emit(OpCode::False);
            }
            LiteralValue::Nil => {
                self.emit(OpCode::Nil);
            }
        }
        Ok(())
    }

    fn visit_unary_expr(&mut self, operator: &Token, right: &Expr) -> CResult {
        if self.opt.constant_folding {
            if let Some(val) = self.try_eval_unary(&operator.token_type, right) {
                self.line = operator.line;
                return self.emit_constant_val(val);
            }
        }

        right.accept(self)?;
        self.line = operator.line;
        match operator.token_type {
            TokenType::Minus => self.emit(OpCode::Negate),
            TokenType::Bang => self.emit(OpCode::Not),
            _ => {
                return Err(
                    self.error(&format!("Unexpected unary operator '{}'.", operator.lexeme))
                );
            }
        };
        Ok(())
    }

    fn visit_binary_expr(&mut self, left: &Expr, operator: &Token, right: &Expr) -> CResult {
        if self.opt.constant_folding {
            if let Some(val) = self.try_eval_binary(left, &operator.token_type, right) {
                self.line = operator.line;
                return self.emit_constant_val(val);
            }
        }

        if self.opt.superinstructions {
            if matches!(operator.token_type, TokenType::Plus | TokenType::Less) {
                if let Some(name) = extract_var_name(left) {
                    if let Some(slot) = self.state().resolve_local(&name.lexeme) {
                        if slot <= u8::MAX as usize {
                            let const_val = if self.opt.constant_folding {
                                self.try_eval_const(right)
                            } else {
                                extract_literal(right)
                            };

                            if let Some(val) = const_val {
                                let k = self.make_constant(val)?;
                                self.line = operator.line;
                                if operator.token_type == TokenType::Plus {
                                    self.emit(OpCode::AddLocalConst(slot as u8, k));
                                } else {
                                    self.emit(OpCode::LessLocalConst(slot as u8, k));
                                }
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        left.accept(self)?;
        right.accept(self)?;
        self.line = operator.line;
        match operator.token_type {
            TokenType::Plus => {
                self.emit(OpCode::Add);
            }
            TokenType::Minus => {
                self.emit(OpCode::Subtract);
            }
            TokenType::Star => {
                self.emit(OpCode::Multiply);
            }
            TokenType::Slash => {
                self.emit(OpCode::Divide);
            }
            TokenType::Percent => {
                self.emit(OpCode::Modulo);
            }
            TokenType::EqualEqual => {
                self.emit(OpCode::Equal);
            }
            TokenType::Greater => {
                self.emit(OpCode::Greater);
            }
            TokenType::Less => {
                self.emit(OpCode::Less);
            }
            TokenType::BangEqual => {
                self.emit(OpCode::Equal);
                self.emit(OpCode::Not);
            }
            TokenType::GreaterEqual => {
                self.emit(OpCode::GreaterEqual);
            }
            TokenType::LessEqual => {
                self.emit(OpCode::LessEqual);
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

    fn visit_grouping_expr(&mut self, expression: &Expr) -> CResult {
        expression.accept(self)
    }

    fn visit_variable_expr(&mut self, name: &Token) -> CResult {
        self.line = name.line;
        let var = self.resolve_variable(&name.lexeme)?;
        self.emit_get_var(&var, &name.lexeme)
    }

    fn visit_assign_expr(&mut self, name: &Token, value: &Expr) -> CResult {
        value.accept(self)?;
        self.line = name.line;
        match self.resolve_variable(&name.lexeme)? {
            Var::Local(slot) => {
                self.emit(OpCode::SetLocal(slot));
            }
            Var::Upvalue(index) => {
                self.emit(OpCode::SetUpvalue(index));
            }
            Var::Global => {
                let k = self.identifier_constant(&name.lexeme)?;
                if self.opt.indexed_globals {
                    self.emit(OpCode::SetGlobalFast(k));
                } else {
                    self.emit(OpCode::SetGlobal(k));
                }
            }
        }
        Ok(())
    }

    fn visit_logical_expr(&mut self, left: &Expr, operator: &Token, right: &Expr) -> CResult {
        left.accept(self)?;
        self.line = operator.line;
        match operator.token_type {
            TokenType::And => {
                // Si left es falsy, ese valor es el resultado: se saltea right.
                let end = self.emit_jump(OpCode::JumpIfFalse(0));
                self.emit(OpCode::Pop);
                right.accept(self)?;
                self.patch_jump(end)?;
            }
            _ => {
                // `or`: si left es truthy, se saltea right.
                let else_jump = self.emit_jump(OpCode::JumpIfFalse(0));
                let end_jump = self.emit_jump(OpCode::Jump(0));
                self.patch_jump(else_jump)?;
                self.emit(OpCode::Pop);
                right.accept(self)?;
                self.patch_jump(end_jump)?;
            }
        }
        Ok(())
    }

    fn visit_call_expr(&mut self, callee: &Expr, paren: &Token, arguments: &[Expr]) -> CResult {
        if self.opt.inline_caches {
            if let Expr::Get { object, name } = callee {
                object.accept(self)?;
                self.line = name.line;
                let k = self.identifier_constant(&name.lexeme)?;
                self.emit(OpCode::PrepareInvoke(k));
                for arg in arguments {
                    arg.accept(self)?;
                }
                self.line = paren.line;
                if arguments.len() > u8::MAX as usize {
                    return Err(self.error("Can't have more than 255 arguments."));
                }
                self.emit(OpCode::Invoke(arguments.len() as u8));
                return Ok(());
            }
            if let Expr::Super { keyword, method } = callee {
                self.line = keyword.line;
                match self.classes.last() {
                    None => return Err(self.error("Can't use 'super' outside of a class.")),
                    Some(cs) if !cs.has_superclass => {
                        return Err(self.error("Can't use 'super' in a class with no superclass."));
                    }
                    _ => {}
                }
                let this_var = self.resolve_variable("this")?;
                self.emit_get_var(&this_var, "this")?;
                let super_var = self.resolve_variable("super")?;
                self.emit_get_var(&super_var, "super")?;
                self.line = method.line;
                let k = self.identifier_constant(&method.lexeme)?;
                self.emit(OpCode::PrepareSuperInvoke(k));
                for arg in arguments {
                    arg.accept(self)?;
                }
                self.line = paren.line;
                if arguments.len() > u8::MAX as usize {
                    return Err(self.error("Can't have more than 255 arguments."));
                }
                self.emit(OpCode::Invoke(arguments.len() as u8));
                return Ok(());
            }
        }

        callee.accept(self)?;
        for arg in arguments {
            arg.accept(self)?;
        }
        self.line = paren.line;
        if arguments.len() > u8::MAX as usize {
            return Err(self.error("Can't have more than 255 arguments."));
        }
        self.emit(OpCode::Call(arguments.len() as u8));
        Ok(())
    }

    fn visit_get_expr(&mut self, object: &Expr, name: &Token) -> CResult {
        object.accept(self)?;
        self.line = name.line;
        let k = self.identifier_constant(&name.lexeme)?;
        self.emit(OpCode::GetProperty(k));
        Ok(())
    }

    fn visit_set_expr(&mut self, object: &Expr, name: &Token, value: &Expr) -> CResult {
        // 1. Evaluar el objeto receptor
        object.accept(self)?;

        // 2. Verificar que sea instancia antes de evaluar el valor asignado
        self.line = name.line;
        self.emit(OpCode::CheckInstance);

        // 3. Evaluar el valor a asignar (solo se alcanza si era instancia)
        value.accept(self)?;

        // 4. Asignar el campo y dejar el valor en el stack
        self.line = name.line;
        let k = self.identifier_constant(&name.lexeme)?;
        self.emit(OpCode::SetProperty(k));
        Ok(())
    }

    fn visit_assign_modify_expr(
        &mut self,
        name: &Token,
        operator: &Token,
        value: &Expr,
    ) -> CResult {
        // 1. Obtener el valor actual de la variable
        self.line = name.line;
        let var = self.resolve_variable(&name.lexeme)?;
        self.emit_get_var(&var, &name.lexeme)?;

        // 2. Evaluar el lado derecho
        value.accept(self)?;

        // 3. Aplicar la operación binaria compuesta
        self.line = operator.line;
        self.emit_compound_op(operator)?;

        // 4. Asignar el nuevo valor dejando el resultado en el stack
        self.line = name.line;
        match var {
            Var::Local(slot) => {
                self.emit(OpCode::SetLocal(slot));
            }
            Var::Upvalue(index) => {
                self.emit(OpCode::SetUpvalue(index));
            }
            Var::Global => {
                let k = self.identifier_constant(&name.lexeme)?;
                if self.opt.indexed_globals {
                    self.emit(OpCode::SetGlobalFast(k));
                } else {
                    self.emit(OpCode::SetGlobal(k));
                }
            }
        }
        Ok(())
    }

    fn visit_set_modify_expr(
        &mut self,
        object: &Expr,
        name: &Token,
        operator: &Token,
        value: &Expr,
    ) -> CResult {
        // 1. Evaluar el objeto receptor exactamente una vez
        object.accept(self)?;

        // 2. Verificar que sea instancia antes de evaluar el valor o continuar
        self.line = name.line;
        self.emit(OpCode::CheckInstance);

        // 3. Duplicar la referencia al objeto: stack [..., obj, obj]
        self.emit(OpCode::Dup);

        // 4. Obtener el valor actual de la propiedad: stack [..., obj, current_val]
        let k = self.identifier_constant(&name.lexeme)?;
        self.emit(OpCode::GetProperty(k));

        // 5. Evaluar el lado derecho: stack [..., obj, current_val, rhs]
        value.accept(self)?;

        // 6. Aplicar la operación: stack [..., obj, new_val]
        self.line = operator.line;
        self.emit_compound_op(operator)?;

        // 7. Asignar la propiedad dejando new_val en el stack: stack [..., new_val]
        self.line = name.line;
        self.emit(OpCode::SetProperty(k));
        Ok(())
    }

    fn visit_this_expr(&mut self, _expr: &Expr, keyword: &Token) -> CResult {
        self.line = keyword.line;
        if self.classes.is_empty() {
            return Err(self.error("Can't use 'this' outside of a class."));
        }
        let var = self.resolve_variable("this")?;
        self.emit_get_var(&var, "this")?;
        Ok(())
    }

    fn visit_super_expr(&mut self, _expr: &Expr, keyword: &Token, method: &Token) -> CResult {
        self.line = keyword.line;
        match self.classes.last() {
            None => return Err(self.error("Can't use 'super' outside of a class.")),
            Some(cs) if !cs.has_superclass => {
                return Err(self.error("Can't use 'super' in a class with no superclass."));
            }
            _ => {}
        }
        let this_var = self.resolve_variable("this")?;
        self.emit_get_var(&this_var, "this")?;
        let super_var = self.resolve_variable("super")?;
        self.emit_get_var(&super_var, "super")?;
        let k = self.identifier_constant(&method.lexeme)?;
        self.emit(OpCode::GetSuper(k));
        Ok(())
    }

    fn visit_ternary_expr(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: &Expr,
    ) -> CResult {
        condition.accept(self)?;
        let then_jump = self.emit_jump(OpCode::JumpIfFalse(0));
        self.emit(OpCode::Pop);
        then_branch.accept(self)?;
        let else_jump = self.emit_jump(OpCode::Jump(0));
        self.patch_jump(then_jump)?;
        self.emit(OpCode::Pop);
        else_branch.accept(self)?;
        self.patch_jump(else_jump)
    }

    fn visit_list_expr(&mut self, bracket: &Token, elements: &[Expr]) -> CResult {
        for el in elements {
            el.accept(self)?;
        }
        self.line = bracket.line;
        self.emit(OpCode::BuildList(elements.len() as u16));
        Ok(())
    }

    fn visit_index_get_expr(&mut self, object: &Expr, bracket: &Token, index: &Expr) -> CResult {
        object.accept(self)?;
        index.accept(self)?;
        self.line = bracket.line;
        self.emit(OpCode::GetIndex);
        Ok(())
    }

    fn visit_index_set_expr(
        &mut self,
        object: &Expr,
        bracket: &Token,
        index: &Expr,
        value: &Expr,
    ) -> CResult {
        object.accept(self)?;
        index.accept(self)?;
        value.accept(self)?;
        self.line = bracket.line;
        self.emit(OpCode::SetIndex);
        Ok(())
    }

    fn visit_index_set_modify_expr(
        &mut self,
        object: &Expr,
        bracket: &Token,
        index: &Expr,
        operator: &Token,
        value: &Expr,
    ) -> CResult {
        object.accept(self)?;
        index.accept(self)?;
        self.line = bracket.line;
        self.emit(OpCode::PrepareIndexModify);
        value.accept(self)?;
        self.line = operator.line;
        self.emit_compound_op(operator)?;
        self.line = bracket.line;
        self.emit(OpCode::SetIndex);
        Ok(())
    }
}

impl StmtVisitor<CResult> for Compiler {
    fn visit_expression_stmt(&mut self, expression: &Expr) -> CResult {
        if self.opt.superinstructions {
            if let Expr::Assign { name, value } = unwrap_grouping(expression) {
                if let Some(slot) = self.state().resolve_local(&name.lexeme) {
                    value.accept(self)?;
                    self.line = name.line;
                    self.emit(OpCode::SetLocalPop(slot as u16));
                    return Ok(());
                }
            }
        }

        expression.accept(self)?;
        self.emit(OpCode::Pop);
        Ok(())
    }

    fn visit_print_stmt(&mut self, expression: &Expr) -> CResult {
        expression.accept(self)?;
        self.emit(OpCode::Print);
        Ok(())
    }

    fn visit_var_stmt(&mut self, name: &Token, initializer: &Option<Expr>) -> CResult {
        self.line = name.line;
        match initializer {
            Some(init) => init.accept(self)?,
            None => {
                self.emit(OpCode::Nil);
            }
        }
        if self.state().scope_depth == 0 {
            let k = self.identifier_constant(&name.lexeme)?;
            self.emit(OpCode::DefineGlobal(k));
        } else {
            self.add_local(name)?;
        }
        Ok(())
    }

    fn visit_block_stmt(&mut self, statements: &[Stmt]) -> CResult {
        self.begin_scope();
        self.compile_stmt_sequence(statements)?;
        self.end_scope();
        Ok(())
    }

    fn visit_if_stmt(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: &Option<Box<Stmt>>,
    ) -> CResult {
        if self.opt.dead_code_elimination {
            if let Some(val) = self.try_eval_condition(condition) {
                if val.is_truthy() {
                    then_branch.accept(self)?;
                    if let Some(else_stmt) = else_branch {
                        self.validate_dead_stmt(else_stmt)?;
                    }
                    return Ok(());
                } else {
                    self.validate_dead_stmt(then_branch)?;
                    if let Some(else_stmt) = else_branch {
                        else_stmt.accept(self)?;
                    }
                    return Ok(());
                }
            }
        }

        condition.accept(self)?;
        let then_jump = self.emit_jump(OpCode::JumpIfFalse(0));
        self.emit(OpCode::Pop);
        then_branch.accept(self)?;
        let else_jump = self.emit_jump(OpCode::Jump(0));
        self.patch_jump(then_jump)?;
        self.emit(OpCode::Pop);
        if let Some(else_stmt) = else_branch {
            else_stmt.accept(self)?;
        }
        self.patch_jump(else_jump)
    }

    fn visit_while_stmt(&mut self, condition: &Expr, body: &Stmt) -> CResult {
        if self.opt.dead_code_elimination {
            if let Some(val) = self.try_eval_condition(condition) {
                if !val.is_truthy() {
                    let scope_depth = self.state().scope_depth;
                    let mut validator = self.clone();
                    validator.opt.dead_code_elimination = false;
                    validator.state().loop_contexts.push(LoopContext {
                        scope_depth,
                        break_jumps: Vec::new(),
                        continue_jumps: Vec::new(),
                    });
                    body.accept(&mut validator)?;
                    return Ok(());
                }
            }
        }

        let loop_start = self.chunk().code.len();
        condition.accept(self)?;
        let exit_jump = self.emit_jump(OpCode::JumpIfFalse(0));
        self.emit(OpCode::Pop);

        let scope_depth = self.state().scope_depth;
        self.state().loop_contexts.push(LoopContext {
            scope_depth,
            break_jumps: Vec::new(),
            continue_jumps: Vec::new(),
        });

        body.accept(self)?;

        let loop_ctx = self.state().loop_contexts.pop().unwrap();

        // Continue target
        for c_jump in loop_ctx.continue_jumps {
            self.patch_jump(c_jump)?;
        }

        self.emit_loop(loop_start)?;
        self.patch_jump(exit_jump)?;
        self.emit(OpCode::Pop);

        // Break target: condition is already popped on all paths reaching here
        for b_jump in loop_ctx.break_jumps {
            self.patch_jump(b_jump)?;
        }

        Ok(())
    }

    fn visit_break_stmt(&mut self, keyword: &Token) -> CResult {
        self.line = keyword.line;
        let loop_scope = match self.state().loop_contexts.last() {
            Some(ctx) => ctx.scope_depth,
            None => return Err(self.error("Can't use 'break' outside of a loop.")),
        };

        let locals = self.state().locals.clone();
        for local in locals.iter().rev() {
            if local.depth > loop_scope {
                if local.is_captured {
                    self.emit(OpCode::CloseUpvalue);
                } else {
                    self.emit(OpCode::Pop);
                }
            } else {
                break;
            }
        }

        let jump = self.emit_jump(OpCode::Jump(0));
        self.state()
            .loop_contexts
            .last_mut()
            .unwrap()
            .break_jumps
            .push(jump);
        Ok(())
    }

    fn visit_continue_stmt(&mut self, keyword: &Token) -> CResult {
        self.line = keyword.line;
        let loop_scope = match self.state().loop_contexts.last() {
            Some(ctx) => ctx.scope_depth,
            None => return Err(self.error("Can't use 'continue' outside of a loop.")),
        };

        let locals = self.state().locals.clone();
        for local in locals.iter().rev() {
            if local.depth > loop_scope {
                if local.is_captured {
                    self.emit(OpCode::CloseUpvalue);
                } else {
                    self.emit(OpCode::Pop);
                }
            } else {
                break;
            }
        }

        let jump = self.emit_jump(OpCode::Jump(0));
        self.state()
            .loop_contexts
            .last_mut()
            .unwrap()
            .continue_jumps
            .push(jump);
        Ok(())
    }

    fn visit_for_stmt(
        &mut self,
        initializer: &Option<Box<Stmt>>,
        condition: &Option<Expr>,
        increment: &Option<Expr>,
        body: &Stmt,
    ) -> CResult {
        self.begin_scope();

        if let Some(init) = initializer {
            init.accept(self)?;
        }

        if self.opt.dead_code_elimination {
            if let Some(cond) = condition {
                if let Some(val) = self.try_eval_condition(cond) {
                    if !val.is_truthy() {
                        let scope_depth = self.state().scope_depth;
                        let mut validator = self.clone();
                        validator.opt.dead_code_elimination = false;
                        validator.state().loop_contexts.push(LoopContext {
                            scope_depth,
                            break_jumps: Vec::new(),
                            continue_jumps: Vec::new(),
                        });
                        body.accept(&mut validator)?;
                        self.end_scope();
                        return Ok(());
                    }
                }
            }
        }

        let loop_start = self.chunk().code.len();

        let exit_jump = if let Some(cond) = condition {
            cond.accept(self)?;
            let j = self.emit_jump(OpCode::JumpIfFalse(0));
            self.emit(OpCode::Pop);
            Some(j)
        } else {
            None
        };

        let scope_depth = self.state().scope_depth;
        self.state().loop_contexts.push(LoopContext {
            scope_depth,
            break_jumps: Vec::new(),
            continue_jumps: Vec::new(),
        });

        body.accept(self)?;

        let loop_ctx = self.state().loop_contexts.pop().unwrap();

        // Continue target: patch continue jumps to here, before increment
        for c_jump in loop_ctx.continue_jumps {
            self.patch_jump(c_jump)?;
        }

        // Increment
        if let Some(inc) = increment {
            inc.accept(self)?;
            self.emit(OpCode::Pop);
        }

        self.emit_loop(loop_start)?;

        if let Some(j) = exit_jump {
            self.patch_jump(j)?;
            self.emit(OpCode::Pop);
        }

        // Break target: patch break jumps to here
        for b_jump in loop_ctx.break_jumps {
            self.patch_jump(b_jump)?;
        }

        self.end_scope();
        Ok(())
    }

    fn visit_function_stmt(
        &mut self,
        name: &Token,
        params: &[Token],
        body: &Rc<Vec<Stmt>>,
    ) -> CResult {
        self.line = name.line;
        let is_local = self.state().scope_depth > 0;
        if is_local {
            self.add_local(name)?;
        }

        self.compile_function_body(name, params, body, FnKind::Function)?;

        if !is_local {
            let global = self.identifier_constant(&name.lexeme)?;
            self.emit(OpCode::DefineGlobal(global));
        }
        Ok(())
    }

    fn visit_return_stmt(&mut self, keyword: &Token, value: &Option<Expr>) -> CResult {
        self.line = keyword.line;
        if self.fns.len() == 1 {
            return Err(self.error("Can't return from top-level code."));
        }

        if self.state().kind == FnKind::Initializer {
            if value.is_some() {
                return Err(self.error("Can't return a value from an initializer."));
            }
            self.emit(OpCode::GetLocal(0));
            self.emit(OpCode::Return);
            return Ok(());
        }

        match value {
            Some(expr) => expr.accept(self)?,
            None => {
                self.emit(OpCode::Nil);
            }
        }
        self.emit(OpCode::Return);
        Ok(())
    }

    fn visit_class_stmt(
        &mut self,
        name: &Token,
        superclass: &Option<Expr>,
        methods: &[Stmt],
    ) -> CResult {
        self.line = name.line;
        let is_local = self.state().scope_depth > 0;
        let name_constant = self.identifier_constant(&name.lexeme)?;

        if let Some(super_expr) = superclass {
            if is_local {
                self.add_local_name(&name.lexeme)?;
            }

            self.emit(OpCode::Class(name_constant));

            if !is_local {
                self.emit(OpCode::DefineGlobal(name_constant));
            }

            super_expr.accept(self)?;

            self.begin_scope();
            self.add_local_name("super")?;

            if is_local {
                let slot = self.state().resolve_local(&name.lexeme).unwrap();
                self.emit(OpCode::GetLocal(slot as u16));
            } else if self.opt.indexed_globals {
                self.emit(OpCode::GetGlobalFast(name_constant));
            } else {
                self.emit(OpCode::GetGlobal(name_constant));
            }

            self.emit(OpCode::Inherit);

            self.classes.push(ClassState {
                has_superclass: true,
            });

            if !methods.is_empty() {
                if is_local {
                    let slot = self.state().resolve_local(&name.lexeme).unwrap();
                    self.emit(OpCode::GetLocal(slot as u16));
                } else if self.opt.indexed_globals {
                    self.emit(OpCode::GetGlobalFast(name_constant));
                } else {
                    self.emit(OpCode::GetGlobal(name_constant));
                }

                for method in methods {
                    if let Stmt::Function {
                        name: method_name,
                        params,
                        body,
                    } = method
                    {
                        let kind = if method_name.lexeme == "init" {
                            FnKind::Initializer
                        } else {
                            FnKind::Method
                        };
                        self.compile_function_body(method_name, params, body, kind)?;
                        let method_k = self.identifier_constant(&method_name.lexeme)?;
                        self.emit(OpCode::Method(method_k));
                    }
                }

                self.emit(OpCode::Pop);
            }

            self.classes.pop();
            self.end_scope();
        } else {
            if is_local {
                self.add_local_name(&name.lexeme)?;
            }

            self.emit(OpCode::Class(name_constant));

            self.classes.push(ClassState {
                has_superclass: false,
            });

            for method in methods {
                if let Stmt::Function {
                    name: method_name,
                    params,
                    body,
                } = method
                {
                    let kind = if method_name.lexeme == "init" {
                        FnKind::Initializer
                    } else {
                        FnKind::Method
                    };
                    self.compile_function_body(method_name, params, body, kind)?;
                    let method_k = self.identifier_constant(&method_name.lexeme)?;
                    self.emit(OpCode::Method(method_k));
                }
            }

            self.classes.pop();

            if !is_local {
                self.emit(OpCode::DefineGlobal(name_constant));
            }
        }

        Ok(())
    }
}

// ---- optimizaciones: jump simplification ---------------------------

pub fn simplify_chunk_jumps(chunk: &mut Chunk) {
    // 1. Compactar primero eliminando físicamente Jump(0).
    // Si la compactación detecta offsets inválidos o desbordamientos, aborta de forma segura
    // dejando el chunk original completamente intacto.
    if !compact_chunk_jumps(chunk) {
        return;
    }

    // 2. Aplicar Jump Threading sobre el chunk compactado.
    // Sigue cadenas de saltos incondicionales hasta su destino final
    // y redirige JumpIfFalse cuyo destino inmediato sea un Jump incondicional.
    jump_threading(chunk);

    // 3. Pasada final de compactación defensiva por si alguna redirección expuso un nuevo Jump(0).
    compact_chunk_jumps(chunk);
}

fn jump_threading(chunk: &mut Chunk) {
    let len = chunk.code.len();
    for i in 0..len {
        match chunk.code[i] {
            OpCode::Jump(offset) => {
                let target = i + 1 + offset as usize;
                if target <= len {
                    if let Some(final_target) = resolve_jump_chain(chunk, i, target) {
                        if final_target > i {
                            if let Ok(new_offset) = u16::try_from(final_target - (i + 1)) {
                                chunk.code[i] = OpCode::Jump(new_offset);
                            }
                        }
                    }
                }
            }
            OpCode::JumpIfFalse(offset) => {
                let target = i + 1 + offset as usize;
                // Solo redirigir si el destino inmediato es un Jump incondicional dentro del chunk
                if target < len && matches!(chunk.code[target], OpCode::Jump(_)) {
                    if let Some(final_target) = resolve_jump_chain(chunk, i, target) {
                        if final_target > i {
                            if let Ok(new_offset) = u16::try_from(final_target - (i + 1)) {
                                chunk.code[i] = OpCode::JumpIfFalse(new_offset);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn resolve_jump_chain(chunk: &Chunk, origin: usize, mut curr_target: usize) -> Option<usize> {
    let len = chunk.code.len();
    if curr_target > len {
        return None;
    }
    let mut visited = HashSet::new();
    visited.insert(origin);
    visited.insert(curr_target);

    let mut changed = false;

    while curr_target < len {
        if let OpCode::Jump(next_offset) = chunk.code[curr_target] {
            let next_target = curr_target + 1 + next_offset as usize;
            if next_target > len {
                // Offset inválido fuera del chunk: abortar
                return None;
            }
            if next_target <= origin {
                // No permitir convertir un salto hacia adelante en retroceso.
                // Estrategia conservadora: mantener destino original.
                return None;
            }
            if visited.contains(&next_target) {
                // Ciclo detectado: no existe destino terminal no-salto.
                // Estrategia conservadora requerida por auditoría: mantener destino original intacto.
                return None;
            }
            visited.insert(next_target);
            curr_target = next_target;
            changed = true;
        } else {
            break;
        }
    }

    if changed { Some(curr_target) } else { None }
}

fn compact_chunk_jumps(chunk: &mut Chunk) -> bool {
    let old_len = chunk.code.len();
    if old_len == 0 {
        return true;
    }

    // 1. Identificar instrucciones Jump(0) eliminables
    let to_remove: Vec<bool> = chunk
        .code
        .iter()
        .map(|op| matches!(op, OpCode::Jump(0)))
        .collect();

    if !to_remove.iter().any(|&b| b) {
        return true;
    }

    // 2. Construir mapa de supervivencia para instrucciones no eliminadas
    let mut surviving_indices = Vec::with_capacity(old_len);
    let mut new_len = 0;
    for &remove in &to_remove {
        if remove {
            surviving_indices.push(None);
        } else {
            surviving_indices.push(Some(new_len));
            new_len += 1;
        }
    }

    // 3. Construir target_map para cualquier destino de 0..=old_len
    let mut target_map = Vec::with_capacity(old_len + 1);
    for idx in 0..=old_len {
        let mut target = idx;
        while target < old_len && to_remove[target] {
            target += 1;
        }
        if target >= old_len {
            target_map.push(new_len);
        } else {
            target_map.push(surviving_indices[target].expect("índice sobreviviente"));
        }
    }

    // 4. Construir el nuevo chunk de manera temporal
    let mut new_code = Vec::with_capacity(new_len);
    let mut new_lines = Vec::with_capacity(new_len);

    for old_i in 0..old_len {
        if to_remove[old_i] {
            continue;
        }
        let new_i = new_code.len();
        let old_line = chunk.lines[old_i];
        let op = chunk.code[old_i];

        let new_op = match op {
            OpCode::Jump(offset) => {
                let old_target = old_i + 1 + offset as usize;
                if old_target > old_len {
                    return false; // Offset inválido en chunk original, abortar compactación
                }
                let new_target = target_map[old_target];
                if new_target <= new_i {
                    return false; // No puede saltar hacia atrás
                }
                let delta = match u16::try_from(new_target - (new_i + 1)) {
                    Ok(d) => d,
                    Err(_) => return false, // Desborda u16, abortar
                };
                OpCode::Jump(delta)
            }
            OpCode::JumpIfFalse(offset) => {
                let old_target = old_i + 1 + offset as usize;
                if old_target > old_len {
                    return false;
                }
                let new_target = target_map[old_target];
                if new_target <= new_i {
                    return false;
                }
                let delta = match u16::try_from(new_target - (new_i + 1)) {
                    Ok(d) => d,
                    Err(_) => return false,
                };
                OpCode::JumpIfFalse(delta)
            }
            OpCode::Loop(offset) => {
                if (old_i + 1) < offset as usize {
                    return false; // Offset de loop inválido en chunk original, abortar compactación
                }
                let old_target = (old_i + 1) - offset as usize;
                if old_target > old_len {
                    return false;
                }
                let new_target = target_map[old_target];
                if new_target > new_i + 1 {
                    return false; // No puede saltar hacia adelante
                }
                let delta = match u16::try_from((new_i + 1) - new_target) {
                    Ok(d) => d,
                    Err(_) => return false,
                };
                OpCode::Loop(delta)
            }
            other => other,
        };

        new_code.push(new_op);
        new_lines.push(old_line);
    }

    debug_assert_eq!(new_code.len(), new_lines.len());
    chunk.code = new_code;
    chunk.lines = new_lines;
    true
}
