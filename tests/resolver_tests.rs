use rlox::parser::Parser;
use rlox::resolver::Resolver;
use rlox::scanner::Scanner;

#[test]
fn test_resolver_local_variable_resolution() {
    let source = "
    var a = \"global\";
    {
      fun showA() {
        print a;
      }
      showA();
      var a = \"block\";
      showA();
    }
    ";

    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().unwrap();

    let mut resolver = Resolver::new();
    resolver.resolve_stmts(&statements);

    assert!(resolver.errors.is_empty());
    assert!(!resolver.locals.is_empty());
}

#[test]
fn test_resolver_error_redefining_variable_in_same_scope() {
    let source = "
    {
      var a = 1;
      var a = 2;
    }
    ";

    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().unwrap();

    let mut resolver = Resolver::new();
    resolver.resolve_stmts(&statements);

    assert_eq!(resolver.errors.len(), 1);
    assert!(resolver.errors[0].contains("Already a variable with this name in this scope."));
}

#[test]
fn test_resolver_error_return_from_top_level() {
    let source = "return \"at top level\";";

    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().unwrap();

    let mut resolver = Resolver::new();
    resolver.resolve_stmts(&statements);

    assert_eq!(resolver.errors.len(), 1);
    assert!(resolver.errors[0].contains("Can't return from top-level code."));
}

#[test]
fn test_resolver_error_read_in_own_initializer() {
    let source = "
    {
      var a = a;
    }
    ";

    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().unwrap();

    let mut resolver = Resolver::new();
    resolver.resolve_stmts(&statements);

    assert_eq!(resolver.errors.len(), 1);
    assert!(resolver.errors[0].contains("Can't read local variable in its own initializer."));
}
