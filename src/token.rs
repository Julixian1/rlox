#[derive(Debug, Clone, PartialEq)]
pub enum TokenType { 
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Comma,
    Dot,
    Minus,
    Plus,
    Semicolon,
    Star,
    Percent,
    Slash,

    // Tokens de uno o dos caracteres
    Bang,
    BangEqual,
    Equal,
    EqualEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,

    // Literales
    Identifier,
    String,
    Number,

    // Palabras clave
    And,
    Else,
    False,
    Fun,
    For,
    If,
    Nil,
    Or,
    Print,
    Return,
    True,
    Var,
    While,

    // Fin de archivo
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Literal{
    String(String),
    Number(f64),
    Boolean(bool),
    Nil,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token{
    pub token_type: TokenType,
    pub lexeme: String,
    pub literal: Option<Literal>,
    pub line: usize,
}

impl Token {
    /// Crea una nueva instancia de `Token`.
    ///
    /// # Parámetros
    /// - `token_type`: El tipo del token.
    /// - `lexeme`: El texto original que compone el token.
    /// - `literal`: El valor literal evaluado (si aplica).
    /// - `line`: Número de línea en el código fuente.
    pub fn new(token_type: TokenType, lexeme: String, literal: Option<Literal>, line: usize) -> Self {
        Token{
            token_type,
            lexeme,
            literal,
            line,
        }
    }
}

/// Retorna el `TokenType` correspondiente si la cadena dada es una palabra clave reservada de Lox.
pub fn get_keyword(keyword: &str) -> Option<TokenType>{
    match keyword {
        "and" => Some(TokenType::And),
        "else" => Some(TokenType::Else),
        "false" => Some(TokenType::False),
        "fun" => Some(TokenType::Fun),
        "for" => Some(TokenType::For),
        "if" => Some(TokenType::If),
        "nil" => Some(TokenType::Nil),
        "or" => Some(TokenType::Or),
        "print" => Some(TokenType::Print),
        "return" => Some(TokenType::Return),
        "true" => Some(TokenType::True),
        "var" => Some(TokenType::Var),
        "while" => Some(TokenType::While),
        _ => None,
    }
}