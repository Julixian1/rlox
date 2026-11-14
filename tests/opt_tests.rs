use rlox::checker;
use rlox::compiler::{
    self,
    opt::{OptConfig, OptLevel, OptPass},
};
use rlox::parser::Parser;
use rlox::scanner::Scanner;
use rlox::stmt::Stmt;
use rlox::vm::Vm;

fn parse_and_check(source: &str) -> Vec<Stmt> {
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens().expect("tokens válidos");
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().expect("parse válido");
    let errors = checker::check(&statements);
    assert!(errors.is_empty(), "errores semánticos: {:?}", errors);
    statements
}

fn run_with_opt(source: &str, opt: OptConfig) -> (String, Vec<rlox::chunk::OpCode>) {
    let statements = parse_and_check(source);
    let proto = compiler::compile_with_opt(&statements, opt).expect("compilación exitosa");
    let code = proto.chunk.code.clone();

    let mut output = Vec::new();
    {
        let mut vm = Vm::with_output(&mut output);
        vm.interpret(proto).expect("ejecución exitosa");
    }
    let stdout = String::from_utf8(output)
        .map(|s| s.replace("\r\n", "\n"))
        .unwrap_or_default();
    (stdout, code)
}

#[test]
fn test_e2_value_representation_size() {
    let size_val = std::mem::size_of::<rlox::value::Value>();
    let size_fn = std::mem::size_of::<rlox::compiler::function::LoxFunction>();
    println!("SIZEOF_VALUE = {}", size_val);
    println!("SIZEOF_LOX_FUNCTION = {}", size_fn);
    assert_eq!(size_fn, 120);
    assert_eq!(size_val, 24);
}

#[test]
fn test_opt_config_levels_and_flags() {
    let o0 = OptConfig::from_level(OptLevel::O0);
    assert_eq!(o0.level, OptLevel::O0);
    assert!(!o0.constant_folding);
    assert!(!o0.dead_code_elimination);
    assert!(!o0.peephole);
    assert_eq!(o0.enabled_summary(), "none");

    let o1 = OptConfig::from_level(OptLevel::O1);
    assert_eq!(o1.level, OptLevel::O1);
    assert!(o1.constant_folding);
    assert!(o1.dead_code_elimination);
    assert!(o1.jump_simplification);
    assert!(!o1.peephole);
    assert_eq!(o1.enabled_summary(), "fold+dce+jumps");

    let mut o2 = OptConfig::from_level(OptLevel::O2);
    assert_eq!(o2.level, OptLevel::O2);
    assert!(o2.peephole);
    assert!(o2.superinstructions);
    assert!(o2.inline_caches);
    assert!(o2.indexed_globals);

    // Desactivar flags selectivos
    o2.set_pass(OptPass::Peephole, false);
    assert!(!o2.peephole);
    assert!(o2.constant_folding);

    // Apply flags
    let mut cfg = OptConfig::default();
    cfg.apply_flags("fold,dce", true).unwrap();
    assert!(cfg.constant_folding);
    assert!(cfg.dead_code_elimination);
    assert!(!cfg.peephole);

    cfg.apply_flags("fold", false).unwrap();
    assert!(!cfg.constant_folding);
    assert!(cfg.dead_code_elimination);

    // Flag inválido
    assert!(cfg.apply_flags("invalid_pass", true).is_err());
}

#[test]
fn test_e1_levels_produce_identical_bytecode_and_output() {
    let programs = [
        // Aritmética y locales
        "var a = 2 + 3 * 4; print a;",
        // Condicionales y saltos
        "var x = 10; if (x > 5) { print \"mayor\"; } else { print \"menor\"; }",
        // Bucles while
        "var i = 0; while (i < 3) { print i; i = i + 1; }",
        // Funciones y llamadas
        "fun add(x, y) { return x + y; } print add(10, 20);",
        // Closures
        "fun make() { var v = 42; fun get() { return v; } return get; } print make()();",
        // Clases, this y métodos
        "class Box { init(v) { this.v = v; } get() { return this.v; } } print Box(123).get();",
        // Herencia y super
        "class A { foo() { return 1; } } class B < A { foo() { return super.foo() + 10; } } print B().foo();",
    ];

    let opt_levels = [
        OptConfig::from_level(OptLevel::O0),
        OptConfig::from_level(OptLevel::O1),
        OptConfig::from_level(OptLevel::O2),
    ];

    for source in programs {
        let (out_base, code_base) = run_with_opt(source, opt_levels[0].clone());

        for opt in &opt_levels[1..] {
            let (out_opt, code_opt) = run_with_opt(source, opt.clone());
            assert_eq!(
                out_base, out_opt,
                "Divergencia de salida para programa con nivel {:?}:\n{}",
                opt.level, source
            );
            assert_eq!(
                code_base, code_opt,
                "En E1 el bytecode debe ser estrictamente idéntico entre -O0 y {:?}:\n{}",
                opt.level, source
            );
        }
    }
}

#[test]
fn test_e2_function_rc_identity_and_semantics() {
    let src = r#"
        fun foo() {}
        var f1 = foo;
        var f2 = foo;

        fun make() {
            fun inner() {}
            return inner;
        }
        var m1 = make();
        var m2 = make();
    "#;

    let mut interpreter = rlox::interpreter::Interpreter::new();
    let mut scanner = Scanner::new(src.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse().unwrap();
    let errors = checker::check(&stmts);
    assert!(errors.is_empty());

    let mut resolver = rlox::resolver::Resolver::new();
    resolver.resolve_stmts(&stmts);
    for (ptr, depth) in resolver.locals {
        interpreter.resolve(ptr, depth);
    }

    let result = interpreter.interpret(&stmts);
    assert!(result.is_ok());
}
