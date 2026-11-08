use crate::chunk::{Chunk, OpCode};
use crate::expr::{Expr, ExprVisitor, LiteralValue};
use crate::stmt::{Stmt, StmtVisitor};
use crate::token::{Token, TokenType};
use crate::value::Value;
use std::rc::Rc;
 
type CResult = Result<(), String>;
 
struct Local {
    name: String,
    depth: usize,
}
 
pub struct Compiler {
    chunk: Chunk,
    locals: Vec<Local>,
    scope_depth: usize,
    line: usize,
}
 
pub fn compile(statements: &[Stmt]) -> Result<Chunk, String> {
    Compiler::new().compile(statements)
}
 
impl Compiler {
    pub fn new() -> Self {
        Compiler {
            chunk: Chunk::new(),
            locals: Vec::new(),
            scope_depth: 0,
            line: 1,
        }
    }
 
    pub fn compile(mut self, statements: &[Stmt]) -> Result<Chunk, String> {
        for stmt in statements {
            stmt.accept(&mut self)?;
        }
        // Mientras el script no sea una función, termina devolviendo nil.
        self.emit(OpCode::Nil);
        self.emit(OpCode::Return);
        Ok(self.chunk)
    }
 
    fn emit(&mut self, op: OpCode) -> usize {
        self.chunk.write(op, self.line)
    }
 
    fn error(&self, msg: &str) -> String {
        format!("[line {}] Error: {}", self.line, msg)
    }
 
    fn make_constant(&mut self, value: Value) -> Result<u16, String> {
        self.chunk
            .add_constant(value)
            .map_err(|_| self.error("Too many constants in one chunk."))
    }
 
    fn identifier_constant(&mut self, name: &str) -> Result<u16, String> {
        let existing = self
            .chunk
            .constants
            .iter()
            .position(|c| matches!(c, Value::String(s) if s == name));
        match existing {
            Some(i) => Ok(i as u16),
            None => self.make_constant(Value::String(name.to_string())),
        }
    }
 
    fn emit_jump(&mut self, op: OpCode) -> usize {
        self.chunk.emit_jump(op, self.line)
    }
 
    fn patch_jump(&mut self, at: usize) -> CResult {
        self.chunk
            .patch_jump(at)
            .map_err(|_| self.error("Too much code to jump over."))
    }
 
    fn emit_loop(&mut self, loop_start: usize) -> CResult {
        self.chunk
            .emit_loop(loop_start, self.line)
            .map_err(|_| self.error("Loop body too large."))
    }
 
    fn begin_scope(&mut self) {
        self.scope_depth += 1;
    }
 
    fn end_scope(&mut self) {
        self.scope_depth -= 1;
        while self
            .locals
            .last()
            .is_some_and(|l| l.depth > self.scope_depth)
        {
            self.locals.pop();
            self.emit(OpCode::Pop);
        }
    }
 
    fn add_local(&mut self, name: &Token) -> CResult {
        if self.locals.len() > u16::MAX as usize {
            return Err(self.error("Too many local variables."));
        }
        self.locals.push(Local {
            name: name.lexeme.clone(),
            depth: self.scope_depth,
        });
        Ok(())
    }
 
    /// Slot de la variable local más interna con ese nombre, si existe.
    fn resolve_local(&self, name: &str) -> Option<u16> {
        self.locals
            .iter()
            .rposition(|l| l.name == name)
            .map(|i| i as u16)
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
            _ => return Err(self.error(&format!("Unexpected unary operator '{}'.", operator.lexeme))),
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
        match self.resolve_local(&name.lexeme) {
            Some(slot) => {
                self.emit(OpCode::GetLocal(slot));
            }
            None => {
                let k = self.identifier_constant(&name.lexeme)?;
                self.emit(OpCode::GetGlobal(k));
            }
        }
        Ok(())
    }
 
    fn visit_assign_expr(&mut self, name: &Token, value: &Expr) -> CResult {
        value.accept(self)?;
        self.line = name.line;
        match self.resolve_local(&name.lexeme) {
            Some(slot) => {
                self.emit(OpCode::SetLocal(slot));
            }
            None => {
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
                let end = self.emit_jump(OpCode::JumpIfFalse(0));
                self.emit(OpCode::Pop);
                right.accept(self)?;
                self.patch_jump(end)?;
            }
            _ => {
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
        if self.scope_depth == 0 {
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
        let loop_start = self.chunk.code.len();
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
        _params: &[Token],
        _body: &Rc<Vec<Stmt>>,
    ) -> CResult {
        self.line = name.line;
        Err(self.error("Function declarations are not supported by the bytecode compiler yet."))
    }
 
    fn visit_return_stmt(&mut self, keyword: &Token, _value: &Option<Expr>) -> CResult {
        self.line = keyword.line;
        Err(self.error("'return' is not supported by the bytecode compiler yet."))
    }
}
