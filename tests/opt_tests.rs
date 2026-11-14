use rlox::checker;
use rlox::chunk::OpCode;
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
        // Aritmética con variables (no constante)
        "var a = 2; var b = 3; var c = a + b * 4; print c;",
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

    // Para expresiones constantes, si se deshabilita folding (--no-opt=fold), el bytecode de O1 y O2 debe ser idéntico a O0
    let const_src = "var a = 2 + 3 * 4; print a;";
    let (out_base, code_base) = run_with_opt(const_src, OptConfig::from_level(OptLevel::O0));
    for level in [OptLevel::O1, OptLevel::O2] {
        let mut opt = OptConfig::from_level(level);
        opt.apply_flags("fold", false).unwrap();
        let (out_opt, code_opt) = run_with_opt(const_src, opt);
        assert_eq!(out_base, out_opt);
        assert_eq!(code_base, code_opt);
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

#[test]
fn test_e4a_constant_folding_bytecode_and_flags() {
    let src = "print (10 + 20) * 3;";

    // 1. -O0 no debe aplicar folding
    let o0 = OptConfig::from_level(OptLevel::O0);
    let (out_o0, code_o0) = run_with_opt(src, o0);
    assert_eq!(out_o0, "90\n");
    // Debe tener Constant, Constant, Add, Constant, Multiply, Print, Nil, Return
    assert!(code_o0.contains(&OpCode::Add));
    assert!(code_o0.contains(&OpCode::Multiply));

    // 2. -O1 y -O2 deben aplicar folding
    let o1 = OptConfig::from_level(OptLevel::O1);
    let (out_o1, code_o1) = run_with_opt(src, o1);
    assert_eq!(out_o1, "90\n");
    assert!(!code_o1.contains(&OpCode::Add));
    assert!(!code_o1.contains(&OpCode::Multiply));
    assert_eq!(
        code_o1,
        vec![OpCode::Constant(0), OpCode::Print, OpCode::Nil, OpCode::Return]
    );

    let o2 = OptConfig::from_level(OptLevel::O2);
    let (out_o2, code_o2) = run_with_opt(src, o2);
    assert_eq!(out_o2, "90\n");
    assert_eq!(code_o1, code_o2);

    // 3. -O0 --opt=fold habilita la pasada
    let mut o0_fold = OptConfig::from_level(OptLevel::O0);
    o0_fold.apply_flags("fold", true).unwrap();
    let (out_o0_fold, code_o0_fold) = run_with_opt(src, o0_fold);
    assert_eq!(out_o0_fold, "90\n");
    assert_eq!(code_o0_fold, code_o1);

    // 4. -O1 --no-opt=fold deshabilita la pasada
    let mut o1_no_fold = OptConfig::from_level(OptLevel::O1);
    o1_no_fold.apply_flags("fold", false).unwrap();
    let (out_o1_no_fold, code_o1_no_fold) = run_with_opt(src, o1_no_fold);
    assert_eq!(out_o1_no_fold, "90\n");
    assert_eq!(code_o1_no_fold, code_o0);
}

#[test]
fn test_e4a_nested_expressions_and_constant_pool() {
    let src = "print 2 + 3 * 4; print (10 - 4) / 2; print -(-5);";
    let o1 = OptConfig::from_level(OptLevel::O1);
    let stmts = parse_and_check(src);
    let proto = compiler::compile_with_opt(&stmts, o1).expect("compilación");

    // Solo deben existir las 3 constantes finales (14, 3, 5), sin valores intermedios
    assert_eq!(proto.chunk.constants.len(), 3);
    assert_eq!(proto.chunk.constants[0], rlox::value::Value::Number(14.0));
    assert_eq!(proto.chunk.constants[1], rlox::value::Value::Number(3.0));
    assert_eq!(proto.chunk.constants[2], rlox::value::Value::Number(5.0));

    // No debe haber opcodes aritméticos
    assert!(!proto.chunk.code.contains(&OpCode::Add));
    assert!(!proto.chunk.code.contains(&OpCode::Multiply));
    assert!(!proto.chunk.code.contains(&OpCode::Subtract));
    assert!(!proto.chunk.code.contains(&OpCode::Divide));
    assert!(!proto.chunk.code.contains(&OpCode::Negate));
}

#[test]
fn test_e4a_comparisons_and_equality_folding() {
    let src = r#"
        print 10 > 3;
        print 5 <= 2;
        print 4 >= 4;
        print 1 < 2;
        print 10 == 10;
        print 10 != 10;
        print "hola" == "hola";
        print "hola" != "chau";
        print 10 == "10";
        print nil == nil;
        print nil != false;
    "#;
    let o0 = OptConfig::from_level(OptLevel::O0);
    let o1 = OptConfig::from_level(OptLevel::O1);

    let (out_o0, code_o0) = run_with_opt(src, o0);
    let (out_o1, code_o1) = run_with_opt(src, o1);

    assert_eq!(out_o0, out_o1);
    // En O1, todas las comparaciones se resuelven a True o False directamente
    assert!(!code_o1.contains(&OpCode::Greater));
    assert!(!code_o1.contains(&OpCode::Less));
    assert!(!code_o1.contains(&OpCode::GreaterEqual));
    assert!(!code_o1.contains(&OpCode::LessEqual));
    assert!(!code_o1.contains(&OpCode::Equal));
    assert!(code_o0.contains(&OpCode::Greater));
}

#[test]
fn test_e4a_no_folding_on_variables_and_side_effects() {
    let src = r#"
        var x = 10;
        print x + 20;
        print (10 + 20) * x;
    "#;
    let o1 = OptConfig::from_level(OptLevel::O1);
    let stmts = parse_and_check(src);
    let proto = compiler::compile_with_opt(&stmts, o1).expect("compilación");

    // En 'x + 20', 'x' es variable, no se pliega
    // En '(10 + 20) * x', (10 + 20) se pliega a 30, pero el '*' con 'x' no
    assert!(proto.chunk.code.contains(&OpCode::Add)); // del x + 20
    assert!(proto.chunk.code.contains(&OpCode::Multiply)); // del 30 * x
    // Constante 30.0 generada para la subexpresión plegada
    assert!(proto.chunk.constants.contains(&rlox::value::Value::Number(30.0)));
}

#[test]
fn test_e4a_type_errors_preserved_at_runtime() {
    let type_error_programs = [
        "print 1 + \"hola\";",
        "print 10 < \"mundo\";",
        "print -true;",
        "print false * 3;",
        "print 10 % \"dos\";",
    ];

    for src in type_error_programs {
        let stmts = parse_and_check(src);
        // Debe compilar sin error estático tanto en O0 como en O1
        let p_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0));
        let p_o1 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O1));
        assert!(p_o0.is_ok());
        assert!(p_o1.is_ok());

        // Debe fallar en runtime en ambos con el mismo mensaje
        let mut out0 = Vec::new();
        let err0 = Vm::with_output(&mut out0).interpret(p_o0.unwrap()).unwrap_err();

        let mut out1 = Vec::new();
        let err1 = Vm::with_output(&mut out1).interpret(p_o1.unwrap()).unwrap_err();

        assert_eq!(err0, err1, "Error en runtime divergente para: {}", src);
    }
}

#[test]
fn test_e4a_ieee754_special_values_preserved() {
    let src = r#"
        print 0 / 0;
        print 1 / 0;
        print -1 / 0;
        print -0.0;
        print (0 / 0) <= 1;
        print (0 / 0) >= 1;
        print (0 / 0) == (0 / 0);
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let (out_o1, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O1));
    assert_eq!(out_o0, out_o1);
    assert_eq!(
        out_o1,
        "NaN\ninf\n-inf\n-0\nfalse\nfalse\nfalse\n"
    );
}

#[test]
fn test_e4a_output_parity_comprehensive() {
    let programs = [
        "print 1 + 2 + 3 + 4 + 5;",
        "var x = 5; print (2 * 3) + x + (4 * 5);",
        "var a = 10; var b = 20; if (10 > 5) print a + b; else print a - b;",
        "for (var i = 0; i < 2 * 3; i = i + 1) print i;",
        "fun f(x) { return x * (2 + 3); } print f(4);",
    ];

    for src in programs {
        let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
        let (out_o1, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O1));
        let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));
        assert_eq!(out_o0, out_o1, "Paridad O0 vs O1 para:\n{}", src);
        assert_eq!(out_o0, out_o2, "Paridad O0 vs O2 para:\n{}", src);
    }
}

#[test]
fn test_e4a_multiline_runtime_error_lines_preserved() {
    let cases = [
        // Subexpresión plegada a la izquierda, fallo en '+' exterior en línea 4
        (
            "var a = (\n    10 +\n    20\n) + \"hola\";",
            4,
            "Operands must be two numbers or two strings.",
        ),
        // Operador exterior en línea 2 intermedia
        (
            "var a = (10 + 20)\n    +\n    \"hola\";",
            2,
            "Operands must be two numbers or two strings.",
        ),
        // Fallo en '-' exterior en línea 1 con subexpresión derecha plegada
        (
            "var a = \"hola\" -\n    (\n        10 + 20\n    );",
            1,
            "Operands must be numbers.",
        ),
        // Comparación multilínea con subexpresión plegada a la izquierda
        (
            "var a = (\n    10 * 20\n) <\n\"mundo\";",
            3,
            "Operands must be numbers.",
        ),
        // Unario multilínea que luego falla por tipos en '-' en línea 3
        (
            "var a = -(\n    10 + 20\n) - \"hola\";",
            3,
            "Operands must be numbers.",
        ),
    ];

    for (src, expected_line, expected_msg_substr) in cases {
        let stmts = parse_and_check(src);

        let p_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0)).unwrap();
        let p_o1 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O1)).unwrap();

        let mut out0 = Vec::new();
        let err0 = Vm::with_output(&mut out0).interpret(p_o0).unwrap_err();

        let mut out1 = Vec::new();
        let err1 = Vm::with_output(&mut out1).interpret(p_o1).unwrap_err();

        assert_eq!(err0, err1, "Discrepancia entre -O0 y -O1 para:\n{}", src);
        let expected_line_str = format!("[line {}]", expected_line);
        assert!(
            err1.contains(&expected_line_str),
            "Se esperaba línea {} en error: '{}' para:\n{}",
            expected_line,
            err1,
            src
        );
        assert!(
            err1.contains(expected_msg_substr),
            "Se esperaba '{}' en error: '{}'",
            expected_msg_substr,
            err1
        );
    }
}

#[test]
fn test_e4a_compiler_validations_not_bypassed() {
    let compile_error_cases = [
        (
            "var x = 10 + this;",
            "Can't use 'this' outside of a class.",
        ),
        (
            "var x = super.foo() + 20;",
            "Can't use 'super' outside of a class.",
        ),
        (
            "class A { foo() { return super.bar() + 1; } }",
            "Can't use 'super' in a class with no superclass.",
        ),
    ];

    for (src, expected_err) in compile_error_cases {
        let mut scanner = Scanner::new(src.to_string());
        let tokens = scanner.scan_tokens().unwrap();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().unwrap();

        // Tanto -O0 como -O1 deben emitir el error de compilación exacto
        let res_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0));
        let res_o1 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O1));

        assert!(res_o0.is_err());
        assert!(res_o1.is_err());
        let err_o0 = res_o0.unwrap_err();
        let err_o1 = res_o1.unwrap_err();

        assert_eq!(err_o0, err_o1, "Error de compilación divergente");
        assert!(
            err_o1.contains(expected_err),
            "Esperado '{}' pero se obtuvo '{}'",
            expected_err,
            err_o1
        );
    }
}

#[test]
fn test_e4b_dce_conditionals_bytecode_and_flags() {
    let src = r#"
        if (false) {
            print "Nunca";
        } else {
            print "Siempre";
        }
    "#;

    // 1. Con -O0: se emite la condición (False), JumpIfFalse, Pop, rama then, Jump, Pop, rama else
    let o0 = OptConfig::from_level(OptLevel::O0);
    let (out_o0, code_o0) = run_with_opt(src, o0);
    assert_eq!(out_o0, "Siempre\n");
    assert!(code_o0.iter().any(|op| matches!(op, OpCode::JumpIfFalse(_))));
    assert!(code_o0.iter().any(|op| matches!(op, OpCode::Jump(_))));

    // 2. Con -O0 --opt=dce: funciona INDEPENDIENTEMENTE de Constant Folding
    let mut o0_dce = OptConfig::from_level(OptLevel::O0);
    o0_dce.apply_flags("dce", true).unwrap();
    let (out_dce, code_dce) = run_with_opt(src, o0_dce);
    assert_eq!(out_dce, "Siempre\n");
    assert!(!code_dce.iter().any(|op| matches!(op, OpCode::JumpIfFalse(_))));
    assert!(!code_dce.iter().any(|op| matches!(op, OpCode::Jump(_))));
    assert_eq!(
        code_dce,
        vec![
            OpCode::Constant(0), // "Siempre"
            OpCode::Print,
            OpCode::Nil,
            OpCode::Return,
        ]
    );

    // 3. Con -O1 y -O2: DCE activo por defecto
    let o1 = OptConfig::from_level(OptLevel::O1);
    let (out_o1, code_o1) = run_with_opt(src, o1);
    assert_eq!(out_o1, "Siempre\n");
    assert_eq!(code_dce, code_o1);

    // 4. Con -O1 --no-opt=dce: desactiva DCE (mantiene saltos)
    let mut o1_no_dce = OptConfig::from_level(OptLevel::O1);
    o1_no_dce.apply_flags("dce", false).unwrap();
    let (out_no_dce, code_no_dce) = run_with_opt(src, o1_no_dce);
    assert_eq!(out_no_dce, "Siempre\n");
    assert!(code_no_dce.iter().any(|op| matches!(op, OpCode::JumpIfFalse(_))));
}

#[test]
fn test_e4b_dce_if_true_without_else() {
    let src = "if (true) { print \"Si\"; }";

    let o0 = OptConfig::from_level(OptLevel::O0);
    let (out_o0, code_o0) = run_with_opt(src, o0);
    assert_eq!(out_o0, "Si\n");
    assert!(code_o0.iter().any(|op| matches!(op, OpCode::JumpIfFalse(_))));

    let o1 = OptConfig::from_level(OptLevel::O1);
    let (out_o1, code_o1) = run_with_opt(src, o1);
    assert_eq!(out_o1, "Si\n");
    assert!(!code_o1.iter().any(|op| matches!(op, OpCode::JumpIfFalse(_))));
    assert_eq!(
        code_o1,
        vec![
            OpCode::Constant(0), // "Si"
            OpCode::Print,
            OpCode::Nil,
            OpCode::Return,
        ]
    );
}

#[test]
fn test_e4b_dce_if_false_without_else() {
    let src = "if (false) { print \"Nunca\"; }";

    let o0 = OptConfig::from_level(OptLevel::O0);
    let (out_o0, code_o0) = run_with_opt(src, o0);
    assert_eq!(out_o0, "");
    assert!(code_o0.len() > 2); // tiene jumps, pops, constante

    let o1 = OptConfig::from_level(OptLevel::O1);
    let (out_o1, code_o1) = run_with_opt(src, o1);
    assert_eq!(out_o1, "");
    // El cuerpo entero desaparece, solo queda el epílogo del script (Nil, Return)
    assert_eq!(code_o1, vec![OpCode::Nil, OpCode::Return]);
}

#[test]
fn test_e4b_dce_while_false() {
    let src = "while (false) { print \"Nunca\"; }";

    let o0 = OptConfig::from_level(OptLevel::O0);
    let (out_o0, code_o0) = run_with_opt(src, o0);
    assert_eq!(out_o0, "");
    assert!(code_o0.iter().any(|op| matches!(op, OpCode::Loop(_))));

    let o1 = OptConfig::from_level(OptLevel::O1);
    let (out_o1, code_o1) = run_with_opt(src, o1);
    assert_eq!(out_o1, "");
    // Sin bucle, sin jumps, sin código del cuerpo
    assert_eq!(code_o1, vec![OpCode::Nil, OpCode::Return]);
}

#[test]
fn test_e4b_dce_unconditional_return() {
    let src = r#"
        fun f() {
            return 10;
            print "Inalcanzable";
            print "Nunca";
        }
        print f();
    "#;

    let o0 = OptConfig::from_level(OptLevel::O0);
    let (out_o0, _) = run_with_opt(src, o0.clone());
    assert_eq!(out_o0, "10\n");

    let o1 = OptConfig::from_level(OptLevel::O1);
    let (out_o1, _) = run_with_opt(src, o1.clone());
    assert_eq!(out_o1, "10\n");

    let stmts = parse_and_check(src);
    let proto_o0 = compiler::compile_with_opt(&stmts, o0).unwrap();
    let proto_o1 = compiler::compile_with_opt(&stmts, o1).unwrap();

    fn count_prints(proto: &rlox::closure::FnProto) -> usize {
        let mut count = proto.chunk.code.iter().filter(|op| matches!(op, OpCode::Print)).count();
        for c in &proto.chunk.constants {
            if let rlox::value::Value::Proto(p) = c {
                count += count_prints(p);
            }
        }
        count
    }
    // En O0: print f() exterior + 2 prints inalcanzables = 3 prints
    assert_eq!(count_prints(&proto_o0), 3);
    // En O1: solo el print f() exterior = 1 print
    assert_eq!(count_prints(&proto_o1), 1);
}

#[test]
fn test_e4b_dce_preserves_compiler_validations_in_dead_branches() {
    let invalid_programs = [
        ("if (false) { print this; }", "Can't use 'this' outside of a class."),
        ("if (true) { print 1; } else { print this; }", "Can't use 'this' outside of a class."),
        ("while (false) { print super.foo(); }", "Can't use 'super' outside of a class."),
        ("if (false) { return 42; }", "Can't return from top-level code."),
        ("while (false) { return 42; }", "Can't return from top-level code."),
        (
            "class A { foo() { return 1; print super.bar(); } }",
            "Can't use 'super' in a class with no superclass.",
        ),
    ];

    for (src, expected_err) in invalid_programs {
        let mut scanner = Scanner::new(src.to_string());
        let tokens = scanner.scan_tokens().unwrap();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().unwrap();

        let res_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0));
        let res_o1 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O1));

        assert!(res_o0.is_err(), "O0 debió rechazar: {}", src);
        assert!(res_o1.is_err(), "O1 debió rechazar: {}", src);
        let err0 = res_o0.unwrap_err();
        let err1 = res_o1.unwrap_err();
        assert_eq!(err0, err1);
        assert!(err1.contains(expected_err));
    }
}

#[test]
fn test_e4b_dce_nested_and_combined_with_folding() {
    let src = r#"
        if (10 > 3) {
            if (false) {
                print "A";
            } else {
                print "B";
            }
        } else {
            print "C";
        }
    "#;

    let o0 = OptConfig::from_level(OptLevel::O0);
    let (out_o0, _) = run_with_opt(src, o0);

    let o1 = OptConfig::from_level(OptLevel::O1);
    let (out_o1, code_o1) = run_with_opt(src, o1);

    assert_eq!(out_o0, "B\n");
    assert_eq!(out_o1, "B\n");
    assert!(!code_o1.iter().any(|op| matches!(op, OpCode::JumpIfFalse(_) | OpCode::Jump(_))));
    assert_eq!(
        code_o1,
        vec![
            OpCode::Constant(0), // "B"
            OpCode::Print,
            OpCode::Nil,
            OpCode::Return,
        ]
    );
}

#[test]
fn test_e4b_dce_no_elimination_of_reachable_code_or_side_effects() {
    let src = r#"
        fun f(x) {
            if (x > 0) {
                return 10;
            }
            return 20;
        }
        print f(5);
        print f(-1);
    "#;

    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let (out_o1, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O1));

    assert_eq!(out_o0, "10\n20\n");
    assert_eq!(out_o1, "10\n20\n");
}

#[test]
fn test_e4b_dce_duplicate_declarations_in_dead_code_rejected() {
    let duplicate_cases = [
        // Declaraciones duplicadas consecutivas tras return
        (
            r#"
            fun ejemplo() {
                return 10;
                var a = 1;
                var a = 2;
            }
            "#,
            "Already a variable with this name in this scope.",
        ),
        // Declaraciones duplicadas dentro de if(false)
        (
            r#"
            if (false) {
                var x = 1;
                var x = 2;
            }
            "#,
            "Already a variable with this name in this scope.",
        ),
        // Declaraciones duplicadas dentro de while(false)
        (
            r#"
            while (false) {
                var y = 1;
                var y = 2;
            }
            "#,
            "Already a variable with this name in this scope.",
        ),
    ];

    for (src, expected_err) in duplicate_cases {
        let mut scanner = Scanner::new(src.to_string());
        let tokens = scanner.scan_tokens().unwrap();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().unwrap();

        // El checker rechaza el programa idénticamente tanto para O0 como O1
        let errors = checker::check(&stmts);
        assert!(!errors.is_empty(), "El checker debió detectar error en: {}", src);
        assert!(errors[0].contains(expected_err));
    }
}

#[test]
fn test_e4b_dce_consecutive_accumulated_declarations_after_return() {
    let src = r#"
        fun ejemplo() {
            return 42;
            var a = 1;
            var b = a + 2;
            fun inner() {
                return a + b;
            }
            print inner();
        }
        print ejemplo();
    "#;

    // Con O0 y O1 la salida debe ser idéntica: 42
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let (out_o1, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O1));
    assert_eq!(out_o0, "42\n");
    assert_eq!(out_o1, "42\n");
}

#[test]
fn test_e4b_dce_closures_and_upvalues_soundness() {
    let cases = [
        // 1. Variable local capturada únicamente dentro de if (false)
        (
            r#"
            fun outer() {
                var x = 10;
                if (false) {
                    fun dead() { print x; }
                }
                return x;
            }
            print outer();
            "#,
            "10\n",
        ),
        // 2. Variable local capturada únicamente dentro de while (false)
        (
            r#"
            fun outer() {
                var x = 20;
                while (false) {
                    fun dead() { print x; }
                }
                return x;
            }
            print outer();
            "#,
            "20\n",
        ),
        // 3. Variable local capturada tanto por closure alcanzable como por inalcanzable
        (
            r#"
            fun outer() {
                var x = 30;
                if (false) {
                    fun dead() { print x; }
                }
                fun alive() { return x + 5; }
                return alive();
            }
            print outer();
            "#,
            "35\n",
        ),
        // 4. Rama muerta contiene closures anidadas que capturan variables de scopes exteriores
        (
            r#"
            fun outer() {
                var a = 100;
                var b = 200;
                if (false) {
                    fun level1() {
                        fun level2() { return a + b; }
                        return level2();
                    }
                }
                return a + b;
            }
            print outer();
            "#,
            "300\n",
        ),
        // 5. Tras rama eliminada se ejecutan instrucciones que acceden a variables locales o crean closures
        (
            r#"
            fun outer() {
                var a = 10;
                if (false) {
                    var dead_local = 999;
                    fun dead_fn() { print dead_local; }
                }
                var b = 20;
                fun alive() { return a + b; }
                return alive();
            }
            print outer();
            "#,
            "30\n",
        ),
    ];

    for (src, expected_out) in cases {
        let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
        let (out_o1, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O1));

        assert_eq!(out_o0, expected_out);
        assert_eq!(out_o1, expected_out);

        // Además, verificar recursivamente en protos que en O0 existen los saltos y en O1 se eliminaron
        let stmts = parse_and_check(src);
        let p0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0)).unwrap();
        let p1 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O1)).unwrap();

        fn has_jumps(proto: &rlox::closure::FnProto) -> bool {
            if proto.chunk.code.iter().any(|op| matches!(op, OpCode::JumpIfFalse(_) | OpCode::Loop(_))) {
                return true;
            }
            for c in &proto.chunk.constants {
                if let rlox::value::Value::Proto(p) = c {
                    if has_jumps(p) {
                        return true;
                    }
                }
            }
            false
        }

        assert!(has_jumps(&p0), "En O0 deben existir saltos de control para: {}", src);
        assert!(!has_jumps(&p1), "En O1 no deben quedar saltos de control para: {}", src);
    }
}
