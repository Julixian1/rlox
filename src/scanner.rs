use crate::token::{Token, TokenType, Literal, get_keyword};

pub struct Scanner{
    source: Vec<char>,
    tokens: Vec<Token>,
    start: usize,
    current: usize,
    line: usize,
}

impl Scanner {
    pub fn new(source: String) -> Self{
        Scanner{
            source: source.chars().collect(),
            tokens: Vec::new(),
            start: 0,
            current: 0,
            line: 1,
        }
    }

    pub fn scan_tokens(&mut self) -> Result<Vec<Token>,String>{
        while !self.is_at_end(){
            self.start = self.current;
            self.scan_token()?;
        }

        self.tokens.push(Token::new(
            TokenType::Eof,
            "".to_string(),
            None,
            self.line,
        ));

        Ok(self.tokens.clone())
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.source.len()
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            return '\0';
        }
        self.source[self.current]
    }

    fn advance(&mut self) -> char {
        let c = self.peek();
        self.current += 1;
        c
    }

    fn previous(&self) -> char {
        self.source[self.current - 1]
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.is_at_end() || self.source[self.current] != expected {
            return false;
        }
        self.current += 1;
        true
    }

    fn lexeme(&self) -> String {
        self.source[self.start..self.current].iter().collect()
    }

    fn add_token(&mut self, token_type: TokenType, literal: Option<Literal>) {
        let text = self.lexeme();
        self.tokens.push(Token::new(token_type, text, literal, self.line));
    }
    
    fn scan_token(&mut self) -> Result<(), String> {
        let c = self.advance();

        let is_alpha = |chr: char| chr.is_ascii_alphabetic() || chr == '_';
        let is_alphanum = |chr: char| chr.is_ascii_alphanumeric() || chr == '_';

        match c {
            ' ' | '\r' | '\t' => {}
            '\n' => self.line += 1,

            // Tokens de un solo carácter
            '(' => self.add_token(TokenType::LeftParen, None),
            ')' => self.add_token(TokenType::RightParen, None),
            '{' => self.add_token(TokenType::LeftBrace, None),
            '}' => self.add_token(TokenType::RightBrace, None),
            ',' => self.add_token(TokenType::Comma, None),
            '.' => self.add_token(TokenType::Dot, None),
            '-' => self.add_token(TokenType::Minus, None),
            '+' => self.add_token(TokenType::Plus, None),
            ';' => self.add_token(TokenType::Semicolon, None),
            '*' => self.add_token(TokenType::Star, None),
            '%' => self.add_token(TokenType::Percent, None),
            
            '/' => {
                if self.match_char('/') {
                    while self.peek() != '\n' && !self.is_at_end() {
                        self.advance();
                    }
                } else {
                    self.add_token(TokenType::Slash, None);
                }
            }

            // Tokens de uno o dos caracteres
            '!' => {
                let matches = self.match_char('=');
                self.add_token(if matches { TokenType::BangEqual } else { TokenType::Bang }, None);
            }
            '=' => {
                let matches = self.match_char('=');
                self.add_token(if matches { TokenType::EqualEqual } else { TokenType::Equal }, None);
            }
            '<' => {
                let matches = self.match_char('=');
                self.add_token(if matches { TokenType::LessEqual } else { TokenType::Less }, None);
            }
            '>' => {
                let matches = self.match_char('=');
                self.add_token(if matches { TokenType::GreaterEqual } else { TokenType::Greater }, None);
            }

            // Strings con comillas simples ('')
            '\'' => {
                while !self.is_at_end() && self.peek() != '\'' && self.peek() != '\n' {
                    self.advance();
                }

                if self.is_at_end() || self.peek() == '\n' {
                    return Err(format!("Unterminated string: `{}`", self.lexeme()));
                }

                self.advance(); // Consumir la comilla de cierre

                let value: String = self.source[self.start + 1..self.current - 1].iter().collect();
                self.add_token(TokenType::String, Some(Literal::String(value)));
            }

            // Strings con comillas dobles ("")
            '"' => {
                while !self.is_at_end() && self.peek() != '"' {
                    self.advance();
                }

                if self.is_at_end() {
                    return Err(format!("Unterminated string: `{}`", self.lexeme()));
                }

                self.advance(); // Consumir la comilla de cierre

                let value: String = self.source[self.start + 1..self.current - 1].iter().collect();
                self.add_token(TokenType::String, Some(Literal::String(value)));
            }

            // Números
            c_val if c_val.is_ascii_digit() => {
                let mut scanned_dots = 0;

                while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == '.') {
                    if self.peek() == '.' {
                        scanned_dots += 1;
                    }
                    self.advance();
                }

                if scanned_dots > 1 {
                    return Err(format!("Invalid number: `{}`", self.lexeme()));
                }

                if self.previous() == '.' {
                    return Err(format!("Invalid number: `{}`", self.lexeme()));
                }

                let lexeme = self.lexeme();
                let numvalue: f64 = lexeme.parse().map_err(|_| format!("Invalid number: `{}`", lexeme))?;
                self.add_token(TokenType::Number, Some(Literal::Number(numvalue)));
            }

            // Identificadores y palabras reservadas
            c_val if is_alpha(c_val) => {
                while !self.is_at_end() && is_alphanum(self.peek()) {
                    self.advance();
                }

                let text = self.lexeme();
                let token_type = get_keyword(&text).unwrap_or(TokenType::Identifier);
                self.add_token(token_type, None);
            }

            // Carácter inesperado
            _ => {
                return Err(format!("Unexpected character: `{}`", c));
            }
        }

        Ok(())
    }
}