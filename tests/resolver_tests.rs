use rlox::parser::Parser;
use rlox::resolver::Resolver;
use rlox::scanner::Scanner;

fn resolve(source: &str) -> Resolver {
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().unwrap();

    let mut resolver = Resolver::new();
    resolver.resolve_stmts(&statements);
    resolver
}

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

    let resolver = resolve(source);
    assert!(!resolver.locals.is_empty());
}

#[test]
fn test_resolver_records_distance_in_scopes() {
    let resolver = resolve("{ var a = 1; { print a; } }");
    let depths: Vec<usize> = resolver.locals.values().copied().collect();
    assert_eq!(depths, vec![1]);
}

#[test]
fn test_resolver_same_scope_is_distance_zero() {
    let resolver = resolve("{ var a = 1; print a; }");
    let depths: Vec<usize> = resolver.locals.values().copied().collect();
    assert_eq!(depths, vec![0]);
}

#[test]
fn test_resolver_leaves_globals_unresolved() {
    let resolver = resolve("var a = 1; print a; a = 2;");
    assert!(resolver.locals.is_empty());
}

#[test]
fn test_resolver_function_parameters_are_locals() {
    let resolver = resolve("fun f(x) { return x; }");
    let depths: Vec<usize> = resolver.locals.values().copied().collect();
    assert_eq!(depths, vec![0]);
}

#[test]
fn test_resolver_does_not_report_semantic_errors() {
    // Detectar programas inválidos es trabajo de `checker`, no del resolver:
    // acá solo se calcula el binding y se sigue de largo.
    let resolver = resolve("{ var a = 1; var a = 2; }");
    assert_eq!(resolver.locals.len(), 0);
}

#[test]
fn test_resolver_resolves_this_in_method() {
    let resolver = resolve("class Foo { bar() { print this; } }");
    let depths: Vec<usize> = resolver.locals.values().copied().collect();
    // En un método sin parámetros, depth 0 es el ámbito de parámetros y depth 1 es "this".
    assert_eq!(depths, vec![1]);
}

#[test]
fn test_resolver_resolves_super_and_this_in_subclass() {
    let source = "
    class Base {}
    class Sub < Base {
      foo() {
        print super.bar();
        print this;
      }
    }
    ";
    let resolver = resolve(source);
    let mut depths: Vec<usize> = resolver.locals.values().copied().collect();
    depths.sort();
    // 'this' a distancia 1, 'super' a distancia 2 (1 nivel superior a 'this')
    assert_eq!(depths, vec![1, 2]);
}
