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

#[test]
fn test_parse_empty_class() {
    let stmts = parse_source("class Foo {}").unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Class {
            name,
            superclass,
            methods,
        } => {
            assert_eq!(name.lexeme, "Foo");
            assert!(superclass.is_none());
            assert!(methods.is_empty());
        }
        _ => panic!("Expected Class statement"),
    }
}

#[test]
fn test_parse_class_with_methods() {
    let stmts = parse_source("class Foo { bar(a, b) { return a; } baz() {} }").unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Class {
            name,
            superclass,
            methods,
        } => {
            assert_eq!(name.lexeme, "Foo");
            assert!(superclass.is_none());
            assert_eq!(methods.len(), 2);
            match &methods[0] {
                Stmt::Function { name, params, .. } => {
                    assert_eq!(name.lexeme, "bar");
                    assert_eq!(params.len(), 2);
                }
                _ => panic!("Expected Function method"),
            }
            match &methods[1] {
                Stmt::Function { name, params, .. } => {
                    assert_eq!(name.lexeme, "baz");
                    assert_eq!(params.len(), 0);
                }
                _ => panic!("Expected Function method"),
            }
        }
        _ => panic!("Expected Class statement"),
    }
}

#[test]
fn test_parse_class_inheritance() {
    let stmts = parse_source("class Doughnut < Pastry { cook() {} }").unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Class {
            name,
            superclass,
            methods,
        } => {
            assert_eq!(name.lexeme, "Doughnut");
            assert!(superclass.is_some());
            match superclass.as_ref().unwrap() {
                Expr::Variable { name: super_name } => assert_eq!(super_name.lexeme, "Pastry"),
                _ => panic!("Expected Variable superclass expression"),
            }
            assert_eq!(methods.len(), 1);
        }
        _ => panic!("Expected Class statement"),
    }
}

#[test]
fn test_parse_get_and_set_properties() {
    let stmts = parse_source("obj.field = 42; print obj.field;").unwrap();
    assert_eq!(stmts.len(), 2);
    match &stmts[0] {
        Stmt::Expression { expression } => match expression {
            Expr::Set {
                object,
                name,
                value,
            } => {
                assert_eq!(name.lexeme, "field");
                assert!(matches!(**object, Expr::Variable { .. }));
                assert!(matches!(**value, Expr::Literal { .. }));
            }
            _ => panic!("Expected Set expression"),
        },
        _ => panic!("Expected Expression statement"),
    }
    match &stmts[1] {
        Stmt::Print { expression } => match expression {
            Expr::Get { object, name } => {
                assert_eq!(name.lexeme, "field");
                assert!(matches!(**object, Expr::Variable { .. }));
            }
            _ => panic!("Expected Get expression"),
        },
        _ => panic!("Expected Print statement"),
    }
}

#[test]
fn test_parse_super_expression() {
    let stmts = parse_source("class Sub < Base { foo() { super.bar(1, 2); } }").unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Class { methods, .. } => match &methods[0] {
            Stmt::Function { body, .. } => match &body[0] {
                Stmt::Expression { expression } => match expression {
                    Expr::Call {
                        callee, arguments, ..
                    } => {
                        assert_eq!(arguments.len(), 2);
                        match &**callee {
                            Expr::Super { keyword, method } => {
                                assert_eq!(keyword.lexeme, "super");
                                assert_eq!(method.lexeme, "bar");
                            }
                            _ => panic!("Expected Super expression"),
                        }
                    }
                    _ => panic!("Expected Call expression"),
                },
                _ => panic!("Expected Expression statement in body"),
            },
            _ => panic!("Expected Function method"),
        },
        _ => panic!("Expected Class statement"),
    }
}

#[test]
fn test_parse_self_inheritance_error() {
    let result = parse_source("class Foo < Foo {}");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("A class can't inherit from itself.")
    );
}

#[test]
fn test_parse_missing_superclass_error() {
    let result = parse_source("class Foo < {}");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Expected superclass name."));
}
