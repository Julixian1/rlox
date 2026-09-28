use rlox::scanner::Scanner;
use rlox::token::{Literal, TokenType};

#[test]
fn test_scan_empty_source() {
    let mut scanner = Scanner::new("".to_string());
    let tokens = scanner.scan_tokens().unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].token_type, TokenType::Eof);
}

#[test]
fn test_scan_single_character_tokens() {
    let mut scanner = Scanner::new("( ) { } , . - + ; * % /".to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let expected = vec![
        TokenType::LeftParen,
        TokenType::RightParen,
        TokenType::LeftBrace,
        TokenType::RightBrace,
        TokenType::Comma,
        TokenType::Dot,
        TokenType::Minus,
        TokenType::Plus,
        TokenType::Semicolon,
        TokenType::Star,
        TokenType::Percent,
        TokenType::Slash,
        TokenType::Eof,
    ];

    let actual: Vec<TokenType> = tokens.into_iter().map(|t| t.token_type).collect();
    assert_eq!(actual, expected);
}

#[test]
fn test_scan_two_character_operators() {
    let mut scanner = Scanner::new("! != = == < <= > >=".to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let expected = vec![
        TokenType::Bang,
        TokenType::BangEqual,
        TokenType::Equal,
        TokenType::EqualEqual,
        TokenType::Less,
        TokenType::LessEqual,
        TokenType::Greater,
        TokenType::GreaterEqual,
        TokenType::Eof,
    ];

    let actual: Vec<TokenType> = tokens.into_iter().map(|t| t.token_type).collect();
    assert_eq!(actual, expected);
}

#[test]
fn test_scan_literals() {
    let mut scanner = Scanner::new("\"hello world\" 123.45 'single quotes'".to_string());
    let tokens = scanner.scan_tokens().unwrap();

    assert_eq!(tokens[0].token_type, TokenType::String);
    assert_eq!(
        tokens[0].literal,
        Some(Literal::String("hello world".to_string()))
    );

    assert_eq!(tokens[1].token_type, TokenType::Number);
    assert_eq!(tokens[1].literal, Some(Literal::Number(123.45)));

    assert_eq!(tokens[2].token_type, TokenType::String);
    assert_eq!(
        tokens[2].literal,
        Some(Literal::String("single quotes".to_string()))
    );
}

#[test]
fn test_scan_keywords_and_identifiers() {
    let mut scanner = Scanner::new("var count = 10; if (count > 0) return true;".to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let types: Vec<TokenType> = tokens.into_iter().map(|t| t.token_type).collect();

    assert_eq!(
        types,
        vec![
            TokenType::Var,
            TokenType::Identifier,
            TokenType::Equal,
            TokenType::Number,
            TokenType::Semicolon,
            TokenType::If,
            TokenType::LeftParen,
            TokenType::Identifier,
            TokenType::Greater,
            TokenType::Number,
            TokenType::RightParen,
            TokenType::Return,
            TokenType::True,
            TokenType::Semicolon,
            TokenType::Eof,
        ]
    );
}

#[test]
fn test_scan_comments_ignored() {
    let source = "
    // Este es un comentario
    var a = 1; // otro comentario
    ";
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let types: Vec<TokenType> = tokens.into_iter().map(|t| t.token_type).collect();

    assert_eq!(
        types,
        vec![
            TokenType::Var,
            TokenType::Identifier,
            TokenType::Equal,
            TokenType::Number,
            TokenType::Semicolon,
            TokenType::Eof,
        ]
    );
}

#[test]
fn test_scan_unterminated_string_error() {
    let mut scanner = Scanner::new("\"cadena sin cerrar".to_string());
    assert!(scanner.scan_tokens().is_err());
}
