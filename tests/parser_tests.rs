use rlox::expr::{Expr, LiteralValue};
use rlox::parser::Parser;
use rlox::scanner::Scanner;
use rlox::stmt::Stmt;

fn parse_source(source: &str) -> Result<Vec<Stmt>, String> {
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens()?;
    let mut parser = Parser::new(tokens);
    parser.parse()
}

#[test]
fn test_parse_expression_statement() {
    let stmts = parse_source("1 + 2 * 3;").unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Expression { expression } => match expression {
            Expr::Binary {
                left,
                operator,
                right,
            } => {
                assert_eq!(operator.lexeme, "+");
                assert!(
                    matches!(**left, Expr::Literal { value: LiteralValue::Number(n) } if n == 1.0)
                );
                assert!(matches!(**right, Expr::Binary { .. }));
            }
            _ => panic!("Expected Binary expression"),
        },
        _ => panic!("Expected Expression statement"),
    }
}

#[test]
fn test_parse_var_declaration() {
    let stmts = parse_source("var x = 42;").unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Var { name, initializer } => {
            assert_eq!(name.lexeme, "x");
            assert!(initializer.is_some());
            match initializer.as_ref().unwrap() {
                Expr::Literal { value } => assert_eq!(*value, LiteralValue::Number(42.0)),
                _ => panic!("Expected literal initializer"),
            }
        }
        _ => panic!("Expected Var statement"),
    }
}

#[test]
fn test_parse_if_else_statement() {
    let stmts = parse_source("if (x > 0) print true; else print false;").unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            assert!(matches!(condition, Expr::Binary { .. }));
            assert!(matches!(**then_branch, Stmt::Print { .. }));
            assert!(else_branch.is_some());
            assert!(matches!(
                **else_branch.as_ref().unwrap(),
                Stmt::Print { .. }
            ));
        }
        _ => panic!("Expected If statement"),
    }
}

#[test]
fn test_parse_while_statement() {
    let stmts = parse_source("while (i < 10) i = i + 1;").unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::While { condition, body } => {
            assert!(matches!(condition, Expr::Binary { .. }));
            assert!(matches!(**body, Stmt::Expression { .. }));
        }
        _ => panic!("Expected While statement"),
    }
}

#[test]
fn test_parse_function_declaration() {
    let stmts = parse_source("fun add(a, b) { return a + b; }").unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Function { name, params, body } => {
            assert_eq!(name.lexeme, "add");
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].lexeme, "a");
            assert_eq!(params[1].lexeme, "b");
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Stmt::Return { .. }));
        }
        _ => panic!("Expected Function statement"),
    }
}

#[test]
fn test_parse_syntax_error_missing_semicolon() {
    let result = parse_source("var x = 10");
    assert!(result.is_err());
}
