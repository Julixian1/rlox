//! Tipos de tokens léxicos y palabras reservadas del lenguaje Lox.

/// Categorías de tokens reconocidos por el análisis léxico de Lox.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    /// Paréntesis de apertura `(`.
    LeftParen,
    /// Paréntesis de cierre `)`.
    RightParen,
    /// Llave de apertura `{`.
    LeftBrace,
    /// Llave de cierre `}`.
    RightBrace,
    /// Corchete de apertura `[`.
    LeftBracket,
    /// Corchete de cierre `]`.
    RightBracket,
    /// Coma `,`.
    Comma,
    /// Punto `.`.
    Dot,
    /// Signo menos `-`.
    Minus,
    /// Signo más `+`.
    Plus,
    /// Punto y coma `;`.
    Semicolon,
    /// Asterisco `*`.
    Star,
    /// Porcentaje `%` (operador módulo).
    Percent,
    /// Barra diagonal `/`.
    Slash,
    /// Signo de interrogación `?` (operador ternario).
    Question,
    /// Dos puntos `:` (operador ternario).
    Colon,

    /// Negación lógica `!`.
    Bang,
    /// Desigualdad `!=`.
    BangEqual,
    /// Asignación `=`.
    Equal,
    /// Igualdad `==`.
    EqualEqual,
    /// Mayor que `>`.
    Greater,
    /// Mayor o igual que `>=`.
    GreaterEqual,
    /// Menor que `<`.
    Less,
    /// Menor o igual que `<=`.
    LessEqual,
    /// Asignación con suma `+=`.
    PlusEqual,
    /// Asignación con resta `-=`.
    MinusEqual,
    /// Asignación con multiplicación `*=`.
    StarEqual,
    /// Asignación con división `/=`.
    SlashEqual,
    /// Asignación con módulo `%=`.
    PercentEqual,

    /// Identificador de variable o función.
    Identifier,
    /// Literal de cadena de texto.
    String,
    /// Literal numérico en punto flotante.
    Number,

    /// Palabra clave `and`.
    And,
    /// Palabra clave `break`.
    Break,
    /// Palabra clave `class`.
    Class,
    /// Palabra clave `continue`.
    Continue,
    /// Palabra clave `else`.
    Else,
    /// Palabra clave `false`.
    False,
    /// Palabra clave `fun`.
    Fun,
    /// Palabra clave `for`.
    For,
    /// Palabra clave `if`.
    If,
    /// Palabra clave `nil`.
    Nil,
    /// Palabra clave `or`.
    Or,
    /// Palabra clave `print`.
    Print,
    /// Palabra clave `return`.
    Return,
    /// Palabra clave `super`.
    Super,
    /// Palabra clave `this`.
    This,
    /// Palabra clave `true`.
    True,
    /// Palabra clave `var`.
    Var,
    /// Palabra clave `while`.
    While,

    /// Fin de archivo (End-Of-File).
    Eof,
}

/// Representación de valores literales constantes evaluados por el escáner.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Literal {
    /// Literal de cadena.
    String(String),
    /// Literal numérico.
    Number(f64),
    /// Literal booleano.
    Boolean(bool),
    /// Literal nil.
    Nil,
}

/// Representa un token léxico con su tipo, lexema original, valor literal y línea en la fuente.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Tipo sintáctico del token.
    pub token_type: TokenType,
    /// Fragmento exacto del código fuente que compone el token.
    pub lexeme: String,
    /// Valor literal evaluado, si aplica.
    pub literal: Option<Literal>,
    /// Número de línea donde se encontró el token en el archivo fuente.
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
    pub fn new(
        token_type: TokenType,
        lexeme: String,
        literal: Option<Literal>,
        line: usize,
    ) -> Self {
        Token {
            token_type,
            lexeme,
            literal,
            line,
        }
    }
}

/// Retorna el `TokenType` correspondiente si la cadena dada es una palabra clave reservada de Lox.
pub fn get_keyword(keyword: &str) -> Option<TokenType> {
    match keyword {
        "and" => Some(TokenType::And),
        "break" => Some(TokenType::Break),
        "class" => Some(TokenType::Class),
        "continue" => Some(TokenType::Continue),
        "else" => Some(TokenType::Else),
        "false" => Some(TokenType::False),
        "fun" => Some(TokenType::Fun),
        "for" => Some(TokenType::For),
        "if" => Some(TokenType::If),
        "nil" => Some(TokenType::Nil),
        "or" => Some(TokenType::Or),
        "print" => Some(TokenType::Print),
        "return" => Some(TokenType::Return),
        "super" => Some(TokenType::Super),
        "this" => Some(TokenType::This),
        "true" => Some(TokenType::True),
        "var" => Some(TokenType::Var),
        "while" => Some(TokenType::While),
        _ => None,
    }
}
