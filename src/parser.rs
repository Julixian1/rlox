use crate::expr::{Expr, LiteralValue};
use crate::stmt::Stmt;
use crate::token::{Token, TokenType, Literal};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, current: 0 }
    }

    // HELPERS

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn is_at_end(&self) -> bool {
        self.peek().token_type == TokenType::Eof
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn check(&self, token_type: &TokenType) -> bool {
        if self.is_at_end() {
            return false;
        }
        &self.peek().token_type == token_type
    }

    // Avanza si el token actual coincide con alguno de los tipos dados.
    fn match_token(&mut self, types: &[TokenType]) -> bool {
        for t in types {
            if self.check(t) {
                self.advance();
                return true;
            }
        }
        false
    }

    // Consume el token actual si coincide con el tipo esperado, o devuelve error.
    fn consume(&mut self, token_type: &TokenType, message: &str) -> Result<Token, String> {
        if self.check(token_type) {
            Ok(self.advance().clone())
        } else {
            let token = self.peek().clone();
            Err(format!("[line {}] Error at '{}': {}", token.line, token.lexeme, message))
        }
    }

    // PUNTO DE ENTRADA
    pub fn parse(&mut self) -> Result<Vec<Stmt>, String> {
        let mut statements: Vec<Stmt> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        while !self.is_at_end() {
            match self.declaration() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    errors.push(e);
                    self.synchronize();
                }
            }
        }

        if errors.is_empty() {
            Ok(statements)
        } else {
            Err(errors.join("\n"))
        }
    }

    // Sincronización de pánico: avanza hasta el inicio del próximo statement.
    fn synchronize(&mut self) {
        self.advance();

        while !self.is_at_end() {
            if self.previous().token_type == TokenType::Semicolon {
                return;
            }

            match self.peek().token_type {
                TokenType::Fun
                | TokenType::Var
                | TokenType::For
                | TokenType::If
                | TokenType::While
                | TokenType::Print
                | TokenType::Return => return,
                _ => {}
            }

            self.advance();
        }
    }

    // DECLARATIONS

    fn declaration(&mut self) -> Result<Stmt, String> {
        if self.match_token(&[TokenType::Fun]) {
            return self.fun_declaration();
        }
        if self.match_token(&[TokenType::Var]) {
            return self.var_declaration();
        }
        self.statement()
    }

    fn fun_declaration(&mut self) -> Result<Stmt, String> {
        let name = self.consume(&TokenType::Identifier, "Expected function name.")?;

        self.consume(&TokenType::LeftParen, "Expected '(' after function name.")?;

        let mut params: Vec<Token> = Vec::new();
        if !self.check(&TokenType::RightParen) {
            loop {
                if params.len() >= 255 {
                    let token = self.peek().clone();
                    return Err(format!(
                        "[line {}] Error: Can't have more than 255 parameters.",
                        token.line
                    ));
                }
                params.push(self.consume(&TokenType::Identifier, "Expected parameter name.")?);

                if !self.match_token(&[TokenType::Comma]) {
                    break;
                }
            }
        }

        self.consume(&TokenType::RightParen, "Expected ')' after parameters.")?;
        self.consume(&TokenType::LeftBrace, "Expected '{' before function body.")?;

        let body = std::rc::Rc::new(self.block()?);

        Ok(Stmt::Function { name, params, body })
    }

    fn var_declaration(&mut self) -> Result<Stmt, String> {
        let name = self.consume(&TokenType::Identifier, "Expected variable name.")?;

        let initializer = if self.match_token(&[TokenType::Equal]) {
            Some(self.expression()?)
        } else {
            None
        };

        self.consume(&TokenType::Semicolon, "Expected ';' after variable declaration.")?;

        Ok(Stmt::Var { name, initializer })
    }

    // STATEMENTS

    fn statement(&mut self) -> Result<Stmt, String> {
        if self.match_token(&[TokenType::Print]) {
            return self.print_statement();
        }
        if self.match_token(&[TokenType::LeftBrace]) {
            let stmts = self.block()?;
            return Ok(Stmt::Block { statements: stmts });
        }
        if self.match_token(&[TokenType::If]) {
            return self.if_statement();
        }
        if self.match_token(&[TokenType::While]) {
            return self.while_statement();
        }
        if self.match_token(&[TokenType::For]) {
            return self.for_statement();
        }
        if self.match_token(&[TokenType::Return]) {
            return self.return_statement();
        }

        self.expression_statement()
    }

    fn print_statement(&mut self) -> Result<Stmt, String> {
        let expression = self.expression()?;
        self.consume(&TokenType::Semicolon, "Expected ';' after value.")?;
        Ok(Stmt::Print { expression })
    }

    fn expression_statement(&mut self) -> Result<Stmt, String> {
        let expression = self.expression()?;
        self.consume(&TokenType::Semicolon, "Expected ';' after expression.")?;
        Ok(Stmt::Expression { expression })
    }

    // Parsea el contenido de un bloque (asume que '{' ya fue consumido).
    // Retorna las declaraciones internas (sin el nodo Block envolvente).
    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        let mut statements: Vec<Stmt> = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            statements.push(self.declaration()?);
        }

        self.consume(&TokenType::RightBrace, "Expected '}' after block.")?;
        Ok(statements)
    }

    fn if_statement(&mut self) -> Result<Stmt, String> {
        self.consume(&TokenType::LeftParen, "Expected '(' after 'if'.")?;
        let condition = self.expression()?;
        self.consume(&TokenType::RightParen, "Expected ')' after if condition.")?;

        let then_branch = Box::new(self.statement()?);

        let else_branch = if self.match_token(&[TokenType::Else]) {
            Some(Box::new(self.statement()?))
        } else {
            None
        };

        Ok(Stmt::If { condition, then_branch, else_branch })
    }

    fn while_statement(&mut self) -> Result<Stmt, String> {
        self.consume(&TokenType::LeftParen, "Expected '(' after 'while'.")?;
        let condition = self.expression()?;
        self.consume(&TokenType::RightParen, "Expected ')' after while condition.")?;

        let body = Box::new(self.statement()?);

        Ok(Stmt::While { condition, body })
    }

    // Desugaring: for → Block { initializer; While { condition; Block { body; increment } } }
    fn for_statement(&mut self) -> Result<Stmt, String> {
        self.consume(&TokenType::LeftParen, "Expected '(' after 'for'.")?;

        // Inicializador
        let initializer = if self.match_token(&[TokenType::Semicolon]) {
            None
        } else if self.match_token(&[TokenType::Var]) {
            Some(self.var_declaration()?)
        } else {
            Some(self.expression_statement()?)
        };

        // Condición
        let condition = if !self.check(&TokenType::Semicolon) {
            self.expression()?
        } else {
            Expr::Literal { value: LiteralValue::Bool(true) }
        };
        self.consume(&TokenType::Semicolon, "Expected ';' after loop condition.")?;

        // Incremento
        let increment = if !self.check(&TokenType::RightParen) {
            Some(self.expression()?)
        } else {
            None
        };
        self.consume(&TokenType::RightParen, "Expected ')' after for clauses.")?;

        // Body
        let mut body = self.statement()?;

        // Desugaring: agregar incremento al final del body
        if let Some(inc) = increment {
            body = Stmt::Block {
                statements: vec![body, Stmt::Expression { expression: inc }],
            };
        }

        // Envolver en while
        body = Stmt::While {
            condition,
            body: Box::new(body),
        };

        // Envolver con inicializador
        if let Some(init) = initializer {
            body = Stmt::Block {
                statements: vec![init, body],
            };
        }

        Ok(body)
    }

    fn return_statement(&mut self) -> Result<Stmt, String> {
        let keyword = self.previous().clone();

        let value = if !self.check(&TokenType::Semicolon) {
            Some(self.expression()?)
        } else {
            None
        };

        self.consume(&TokenType::Semicolon, "Expected ';' after return value.")?;

        Ok(Stmt::Return { keyword, value })
    }

    // EXPRESSIONS (de menor a mayor precedencia)
    fn expression(&mut self) -> Result<Expr, String> {
        self.assignment()
    }

    fn assignment(&mut self) -> Result<Expr, String> {
        let expr = self.logic_or()?;

        if self.match_token(&[TokenType::Equal]) {
            let value = self.assignment()?;

            if let Expr::Variable { name } = expr {
                return Ok(Expr::Assign {
                    name,
                    value: Box::new(value),
                });
            }

            let equals = self.previous().clone();
            return Err(format!(
                "[line {}] Error at '=': Invalid assignment target.",
                equals.line
            ));
        }

        Ok(expr)
    }

    fn logic_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.logic_and()?;

        while self.match_token(&[TokenType::Or]) {
            let operator = self.previous().clone();
            let right = self.logic_and()?;
            expr = Expr::Logical {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn logic_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.equality()?;

        while self.match_token(&[TokenType::And]) {
            let operator = self.previous().clone();
            let right = self.equality()?;
            expr = Expr::Logical {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn equality(&mut self) -> Result<Expr, String> {
        let mut expr = self.comparison()?;

        while self.match_token(&[TokenType::BangEqual, TokenType::EqualEqual]) {
            let operator = self.previous().clone();
            let right = self.comparison()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn comparison(&mut self) -> Result<Expr, String> {
        let mut expr = self.term()?;

        while self.match_token(&[
            TokenType::Greater,
            TokenType::GreaterEqual,
            TokenType::Less,
            TokenType::LessEqual,
        ]) {
            let operator = self.previous().clone();
            let right = self.term()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn term(&mut self) -> Result<Expr, String> {
        let mut expr = self.factor()?;

        while self.match_token(&[TokenType::Minus, TokenType::Plus]) {
            let operator = self.previous().clone();
            let right = self.factor()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn factor(&mut self) -> Result<Expr, String> {
        let mut expr = self.unary()?;

        while self.match_token(&[TokenType::Slash, TokenType::Star, TokenType::Percent]) {
            let operator = self.previous().clone();
            let right = self.unary()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.match_token(&[TokenType::Bang, TokenType::Minus]) {
            let operator = self.previous().clone();
            let right = self.unary()?;
            return Ok(Expr::Unary {
                operator,
                right: Box::new(right),
            });
        }

        self.call()
    }

    fn call(&mut self) -> Result<Expr, String> {
        let mut expr = self.primary()?;

        loop {
            if self.match_token(&[TokenType::LeftParen]) {
                expr = self.finish_call(expr)?;
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn finish_call(&mut self, callee: Expr) -> Result<Expr, String> {
        let mut arguments: Vec<Expr> = Vec::new();

        if !self.check(&TokenType::RightParen) {
            loop {
                if arguments.len() >= 255 {
                    let token = self.peek().clone();
                    return Err(format!(
                        "[line {}] Error: Can't have more than 255 arguments.",
                        token.line
                    ));
                }
                arguments.push(self.expression()?);

                if !self.match_token(&[TokenType::Comma]) {
                    break;
                }
            }
        }

        let paren = self.consume(&TokenType::RightParen, "Expected ')' after arguments.")?;

        Ok(Expr::Call {
            callee: Box::new(callee),
            paren,
            arguments,
        })
    }

    fn primary(&mut self) -> Result<Expr, String> {
        // Literales booleanos y nil
        if self.match_token(&[TokenType::False]) {
            return Ok(Expr::Literal { value: LiteralValue::Bool(false) });
        }
        if self.match_token(&[TokenType::True]) {
            return Ok(Expr::Literal { value: LiteralValue::Bool(true) });
        }
        if self.match_token(&[TokenType::Nil]) {
            return Ok(Expr::Literal { value: LiteralValue::Nil });
        }

        // Literales numéricos y strings
        if self.match_token(&[TokenType::Number]) {
            let token = self.previous().clone();
            if let Some(Literal::Number(n)) = token.literal {
                return Ok(Expr::Literal { value: LiteralValue::Number(n) });
            }
            return Err(format!("[line {}] Error: Invalid number literal.", token.line));
        }
        if self.match_token(&[TokenType::String]) {
            let token = self.previous().clone();
            if let Some(Literal::String(s)) = token.literal {
                return Ok(Expr::Literal { value: LiteralValue::StringVal(s) });
            }
            return Err(format!("[line {}] Error: Invalid string literal.", token.line));
        }

        // Identificadores
        if self.match_token(&[TokenType::Identifier]) {
            return Ok(Expr::Variable { name: self.previous().clone() });
        }

        // Agrupamiento
        if self.match_token(&[TokenType::LeftParen]) {
            let expr = self.expression()?;
            self.consume(&TokenType::RightParen, "Expected ')' after expression.")?;
            return Ok(Expr::Grouping { expression: Box::new(expr) });
        }

        // Error: token inesperado
        let token = self.peek().clone();
        Err(format!(
            "[line {}] Error at '{}': Expected expression.",
            token.line, token.lexeme
        ))
    }
}
