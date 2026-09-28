use rlox::expr::{Expr, LiteralValue};
use rlox::token::{Token, TokenType};

#[test]
fn test_literal_value_display() {
    assert_eq!(format!("{}", LiteralValue::Number(3.14)), "3.14");
    assert_eq!(
        format!("{}", LiteralValue::StringVal("test".to_string())),
        "\"test\""
    );
    assert_eq!(format!("{}", LiteralValue::Bool(true)), "true");
    assert_eq!(format!("{}", LiteralValue::Nil), "nil");
}

#[test]
fn test_expr_display_formatting() {
    let minus_token = Token::new(TokenType::Minus, "-".to_string(), None, 1);
    let star_token = Token::new(TokenType::Star, "*".to_string(), None, 1);

    // Representa (-123) * (45.67) en formato prefijo -> (* (- 123) 45.67)
    let expression = Expr::Binary {
        left: Box::new(Expr::Unary {
            operator: minus_token,
            right: Box::new(Expr::Literal {
                value: LiteralValue::Number(123.0),
            }),
        }),
        operator: star_token,
        right: Box::new(Expr::Grouping {
            expression: Box::new(Expr::Literal {
                value: LiteralValue::Number(45.67),
            }),
        }),
    };

    assert_eq!(format!("{}", expression), "(* (- 123) (group 45.67))");
}
