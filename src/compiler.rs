use crate::chunk::{Chunk, OpCode};
use crate::closure::{FnProto, UpvalueDesc};
use crate::expr::{Expr, ExprVisitor, LiteralValue};
use crate::stmt::{Stmt, StmtVisitor};
use crate::token::{Token, TokenType};
use crate::value::Value;
use std::rc::Rc;
 
type CResult = Result<(), String>;
 
struct Local {
    name: String,
    depth: usize,
    is_captured: bool,
}
 
struct FnState {
    proto: FnProto,
    locals: Vec<Local>,
    scope_depth: usize,
}
 
impl FnState {
    fn new(name: Option<String>) -> Self {
        FnState {
            proto: FnProto::new(name),
            locals: vec![Local {
                name: String::new(),
                depth: 0,
                is_captured: false,
            }],
            scope_depth: 0,
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
 
pub struct Compiler {
    fns: Vec<FnState>,
    line: usize,
}
 
pub fn compile(statements: &[Stmt]) -> Result<Rc<FnProto>, String> {
    Compiler::new().compile(statements)
}
 
impl Compiler {
    pub fn new() -> Self {
        Compiler {
            fns: vec![FnState::new(None)],
            line: 1,
        }
    }
 
    pub fn compile(mut self, statements: &[Stmt]) -> Result<Rc<FnProto>, String> {
        for stmt in statements {
            stmt.accept(&mut self)?;
        }
        self.emit(OpCode::Nil);
        self.emit(OpCode::Return);
        let script = self.fns.pop().expect("falta el estado del script");
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
 
    fn add_local(&mut self, name: &Token) -> CResult {
        if self.state().locals.len() > u16::MAX as usize {
            return Err(self.error("Too many local variables."));
        }
        let depth = self.state().scope_depth;
        self.state().locals.push(Local {
            name: name.lexeme.clone(),
            depth,
            is_captured: false,
        });
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
        right.accept(self)?;
        self.line = operator.line;
        match operator.token_type {
            TokenType::Minus => self.emit(OpCode::Negate),
            TokenType::Bang => self.emit(OpCode::Not),
            _ => {
                return Err(self.error(&format!(
                    "Unexpected unary operator '{}'.",
                    operator.lexeme
                )));
            }
        };
        Ok(())
    }
 
    fn visit_binary_expr(&mut self, left: &Expr, operator: &Token, right: &Expr) -> CResult {
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
                self.emit(OpCode::Less);
                self.emit(OpCode::Not);
            }
            TokenType::LessEqual => {
                self.emit(OpCode::Greater);
                self.emit(OpCode::Not);
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
        match self.resolve_variable(&name.lexeme)? {
            Var::Local(slot) => {
                self.emit(OpCode::GetLocal(slot));
            }
            Var::Upvalue(index) => {
                self.emit(OpCode::GetUpvalue(index));
            }
            Var::Global => {
                let k = self.identifier_constant(&name.lexeme)?;
                self.emit(OpCode::GetGlobal(k));
            }
        }
        Ok(())
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
                self.emit(OpCode::SetGlobal(k));
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

    fn visit_get_expr(&mut self, _object: &Expr, name: &Token) -> CResult {
        self.line = name.line;
        Err(self.error("Classes and inheritance are not supported in VM backend."))
    }

    fn visit_set_expr(&mut self, _object: &Expr, name: &Token, _value: &Expr) -> CResult {
        self.line = name.line;
        Err(self.error("Classes and inheritance are not supported in VM backend."))
    }

    fn visit_this_expr(&mut self, _expr: &Expr, keyword: &Token) -> CResult {
        self.line = keyword.line;
        Err(self.error("Classes and inheritance are not supported in VM backend."))
    }

    fn visit_super_expr(&mut self, _expr: &Expr, keyword: &Token, _method: &Token) -> CResult {
        self.line = keyword.line;
        Err(self.error("Classes and inheritance are not supported in VM backend."))
    }
}
 
impl StmtVisitor<CResult> for Compiler {
    fn visit_expression_stmt(&mut self, expression: &Expr) -> CResult {
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
        for stmt in statements {
            stmt.accept(self)?;
        }
        self.end_scope();
        Ok(())
    }
 
    fn visit_if_stmt(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: &Option<Box<Stmt>>,
    ) -> CResult {
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
        let loop_start = self.chunk().code.len();
        condition.accept(self)?;
        let exit_jump = self.emit_jump(OpCode::JumpIfFalse(0));
        self.emit(OpCode::Pop);
        body.accept(self)?;
        self.emit_loop(loop_start)?;
        self.patch_jump(exit_jump)?;
        self.emit(OpCode::Pop);
        Ok(())
    }
 
    fn visit_function_stmt(
        &mut self,
        name: &Token,
        params: &[Token],
        body: &Rc<Vec<Stmt>>,
    ) -> CResult {
        self.line = name.line;
        if params.len() > u8::MAX as usize {
            return Err(self.error("Can't have more than 255 parameters."));
        }
 
        let is_local = self.state().scope_depth > 0;
        if is_local {
            self.add_local(name)?;
        }
 
        let mut state = FnState::new(Some(name.lexeme.clone()));
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
 
        for stmt in body.iter() {
            stmt.accept(self)?;
        }
        
        self.emit(OpCode::Nil);
        self.emit(OpCode::Return);
 
        let state = self.fns.pop().expect("falta el estado de la función");
        self.line = name.line;
        let k = self.make_constant(Value::Proto(Rc::new(state.proto)))?;
        self.emit(OpCode::Closure(k));
 
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
        _superclass: &Option<Expr>,
        _methods: &[Stmt],
    ) -> CResult {
        self.line = name.line;
        Err(self.error("Classes and inheritance are not supported in VM backend."))
    }
}