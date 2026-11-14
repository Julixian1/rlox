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
            let (out_opt, _code_opt) = run_with_opt(source, opt.clone());
            assert_eq!(
                out_base, out_opt,
                "Divergencia de salida para programa con nivel {:?}:\n{}",
                opt.level, source
            );

            let mut opt_no_opts = opt.clone();
            opt_no_opts.apply_flags("fold", false).unwrap();
            opt_no_opts.apply_flags("dce", false).unwrap();
            opt_no_opts.apply_flags("jumps", false).unwrap();
            opt_no_opts.apply_flags("invoke", false).unwrap();
            opt_no_opts.apply_flags("globals", false).unwrap();
            let (_, code_no_opts) = run_with_opt(source, opt_no_opts);
            assert_eq!(
                code_base, code_no_opts,
                "Con optimizaciones desactivadas el bytecode debe ser estrictamente idéntico entre -O0 y {:?}:\n{}",
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
        opt.apply_flags("globals", false).unwrap();
        opt.apply_flags("invoke", false).unwrap();
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

// =========================================================================
// Tests E4.C: Jump Simplification
// =========================================================================

#[test]
fn test_e4c_1_jump_zero_isolated() {
    let mut chunk = rlox::chunk::Chunk::new();
    chunk.write(OpCode::Print, 1);
    chunk.write(OpCode::Jump(0), 1);
    chunk.write(OpCode::Return, 2);

    compiler::simplify_chunk_jumps(&mut chunk);

    assert_eq!(chunk.code, vec![OpCode::Print, OpCode::Return]);
    assert_eq!(chunk.lines, vec![1, 2]);
}

#[test]
fn test_e4c_2_multiple_consecutive_jump_zero() {
    let mut chunk = rlox::chunk::Chunk::new();
    chunk.write(OpCode::Print, 1);
    chunk.write(OpCode::Jump(0), 1);
    chunk.write(OpCode::Jump(0), 1);
    chunk.write(OpCode::Jump(0), 2);
    chunk.write(OpCode::Return, 3);

    compiler::simplify_chunk_jumps(&mut chunk);

    assert_eq!(chunk.code, vec![OpCode::Print, OpCode::Return]);
    assert_eq!(chunk.lines, vec![1, 3]);
}

#[test]
fn test_e4c_3_jump_targeting_eliminated_instruction() {
    let mut chunk = rlox::chunk::Chunk::new();
    chunk.write(OpCode::Jump(1), 1);
    chunk.write(OpCode::Print, 2);
    chunk.write(OpCode::Jump(0), 3);
    chunk.write(OpCode::Return, 4);

    compiler::simplify_chunk_jumps(&mut chunk);

    assert_eq!(
        chunk.code,
        vec![OpCode::Jump(1), OpCode::Print, OpCode::Return]
    );
    assert_eq!(chunk.lines, vec![1, 2, 4]);
}

#[test]
fn test_e4c_4_jump_targeting_end_of_chunk() {
    let mut chunk = rlox::chunk::Chunk::new();
    chunk.write(OpCode::Jump(1), 1);
    chunk.write(OpCode::Print, 2);

    compiler::simplify_chunk_jumps(&mut chunk);

    assert_eq!(chunk.code, vec![OpCode::Jump(1), OpCode::Print]);
}

#[test]
fn test_e4c_5_jump_chain_longer_than_16() {
    let mut chunk = rlox::chunk::Chunk::new();
    // 20 saltos Jump(1) intercalados con Print:
    // Cada salto apunta al siguiente salto (0 -> 2 -> 4 -> ... -> 40).
    for i in 0..20 {
        chunk.write(OpCode::Jump(1), (i * 2 + 1) as usize);
        chunk.write(OpCode::Print, (i * 2 + 2) as usize);
    }
    chunk.write(OpCode::Return, 41);

    compiler::simplify_chunk_jumps(&mut chunk);

    // Todos los saltos deben redirigirse directamente al Return final (índice 40).
    // Para el índice 0: target = 40, offset = 40 - (0 + 1) = 39.
    assert_eq!(chunk.code[0], OpCode::Jump(39));
    // Para el índice 2: target = 40, offset = 40 - (2 + 1) = 37.
    assert_eq!(chunk.code[2], OpCode::Jump(37));
    assert_eq!(chunk.code[40], OpCode::Return);
}

#[test]
fn test_e4c_6_cyclic_jump_chain() {
    let mut chunk = rlox::chunk::Chunk::new();
    chunk.write(OpCode::Jump(1), 1);
    chunk.write(OpCode::Print, 2);
    chunk.write(OpCode::Return, 3);

    compiler::simplify_chunk_jumps(&mut chunk);
    assert_eq!(chunk.code.len(), 3);
}

#[test]
fn test_e4c_7_forward_and_backward_jumps() {
    let mut chunk = rlox::chunk::Chunk::new();
    chunk.write(OpCode::Print, 1);
    chunk.write(OpCode::Jump(0), 2);
    chunk.write(OpCode::Loop(2), 3);

    compiler::simplify_chunk_jumps(&mut chunk);

    assert_eq!(chunk.code, vec![OpCode::Print, OpCode::Loop(1)]);
    assert_eq!(chunk.lines, vec![1, 3]);
}

#[test]
fn test_e4c_8_jump_if_false_with_intermediate_jump() {
    let mut chunk = rlox::chunk::Chunk::new();
    chunk.write(OpCode::JumpIfFalse(1), 1);
    chunk.write(OpCode::Print, 2);
    chunk.write(OpCode::Jump(1), 3);
    chunk.write(OpCode::Pop, 4);
    chunk.write(OpCode::Return, 5);

    compiler::simplify_chunk_jumps(&mut chunk);

    assert_eq!(chunk.code[0], OpCode::JumpIfFalse(3));
}

#[test]
fn test_e4c_9_nested_conditionals() {
    let src = r#"
        var x = 10;
        var y = 20;
        if (x > 5) {
            if (y > 15) {
                print "ambos";
            } else {
                print "solo x";
            }
        } else {
            print "ninguno";
        }
    "#;

    let o0 = OptConfig::from_level(OptLevel::O0);
    let mut o0_jumps = OptConfig::from_level(OptLevel::O0);
    o0_jumps.apply_flags("jumps", true).unwrap();

    let (out_o0, _) = run_with_opt(src, o0);
    let (out_jumps, _) = run_with_opt(src, o0_jumps);

    assert_eq!(out_o0, "ambos\n");
    assert_eq!(out_jumps, "ambos\n");
}

#[test]
fn test_e4c_10_nested_loops() {
    let src = r#"
        var sum = 0;
        var i = 0;
        while (i < 3) {
            var j = 0;
            while (j < 3) {
                sum = sum + 1;
                j = j + 1;
            }
            i = i + 1;
        }
        print sum;
    "#;

    let o0 = OptConfig::from_level(OptLevel::O0);
    let mut o0_jumps = OptConfig::from_level(OptLevel::O0);
    o0_jumps.apply_flags("jumps", true).unwrap();

    let (out_o0, _) = run_with_opt(src, o0);
    let (out_jumps, _) = run_with_opt(src, o0_jumps);

    assert_eq!(out_o0, "9\n");
    assert_eq!(out_jumps, "9\n");
}

#[test]
fn test_e4c_11_logical_operators_and_or() {
    let src = r#"
        print 1 and 2 and 3;
        print false and "no";
        print false or nil or "si";
        print true or "no";
    "#;

    let o0 = OptConfig::from_level(OptLevel::O0);
    let mut o0_jumps = OptConfig::from_level(OptLevel::O0);
    o0_jumps.apply_flags("jumps", true).unwrap();

    let (out_o0, _) = run_with_opt(src, o0);
    let (out_jumps, _) = run_with_opt(src, o0_jumps);

    assert_eq!(out_o0, "3\nfalse\nsi\ntrue\n");
    assert_eq!(out_jumps, "3\nfalse\nsi\ntrue\n");
}

#[test]
fn test_e4c_12_closures_and_upvalues() {
    let src = r#"
        fun make() {
            var count = 0;
            fun inc() {
                count = count + 1;
                return count;
            }
            return inc;
        }
        var c = make();
        print c();
        print c();
    "#;

    let o0 = OptConfig::from_level(OptLevel::O0);
    let mut o0_jumps = OptConfig::from_level(OptLevel::O0);
    o0_jumps.apply_flags("jumps", true).unwrap();

    let (out_o0, _) = run_with_opt(src, o0);
    let (out_jumps, _) = run_with_opt(src, o0_jumps);

    assert_eq!(out_o0, "1\n2\n");
    assert_eq!(out_jumps, "1\n2\n");
}

#[test]
fn test_e4c_13_error_lines_preserved() {
    let src = "var x = 10;\nif (x > 5) {\n    print 1 + \"hola\";\n}";

    let o0 = OptConfig::from_level(OptLevel::O0);
    let mut o0_jumps = OptConfig::from_level(OptLevel::O0);
    o0_jumps.apply_flags("jumps", true).unwrap();

    let stmts = parse_and_check(src);
    let p_o0 = compiler::compile_with_opt(&stmts, o0).unwrap();
    let p_jumps = compiler::compile_with_opt(&stmts, o0_jumps).unwrap();

    let mut out0 = Vec::new();
    let err0 = Vm::with_output(&mut out0).interpret(p_o0).unwrap_err();

    let mut out1 = Vec::new();
    let err1 = Vm::with_output(&mut out1).interpret(p_jumps).unwrap_err();

    assert_eq!(err0, err1);
    assert!(err1.contains("[line 3]"));
}

#[test]
fn test_e4c_14_optimization_flags() {
    let o0 = OptConfig::from_level(OptLevel::O0);
    assert!(!o0.jump_simplification);

    let o1 = OptConfig::from_level(OptLevel::O1);
    assert!(o1.jump_simplification);

    let mut o0_jumps = OptConfig::from_level(OptLevel::O0);
    o0_jumps.apply_flags("jumps", true).unwrap();
    assert!(o0_jumps.jump_simplification);

    let mut o1_no_jumps = OptConfig::from_level(OptLevel::O1);
    o1_no_jumps.apply_flags("jumps", false).unwrap();
    assert!(!o1_no_jumps.jump_simplification);
}

#[test]
fn test_e4c_15_output_parity_comprehensive() {
    let programs = [
        "for (var i = 0; i < 5; i = i + 1) { print i; }",
        "var x = 0; while (x < 4) { if (x == 2) print \"dos\"; else print x; x = x + 1; }",
        "class A { foo(n) { if (n > 0) return this.foo(n - 1) + 1; return 0; } } print A().foo(3);",
    ];

    for src in programs {
        let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
        let mut o0_jumps = OptConfig::from_level(OptLevel::O0);
        o0_jumps.apply_flags("jumps", true).unwrap();
        let (out_jumps, _) = run_with_opt(src, o0_jumps);
        assert_eq!(out_o0, out_jumps, "Paridad para: {}", src);
    }
}

#[test]
fn test_e4c_16_loop_targeting_eliminated_jump_zero() {
    let mut chunk = rlox::chunk::Chunk::new();
    chunk.write(OpCode::Nil, 1);     // 0
    chunk.write(OpCode::Jump(0), 2); // 1 (eliminado)
    chunk.write(OpCode::Print, 3);   // 2 -> nuevo 1
    chunk.write(OpCode::Loop(3), 4); // 3 (apunta a 1: 4 - 3 = 1) -> nuevo 2

    compiler::simplify_chunk_jumps(&mut chunk);

    // En el chunk compactado:
    // 0: Nil
    // 1: Print
    // 2: Loop(2) -> apunta a nuevo índice 1 (Print): (2 + 1) - 2 = 1.
    assert_eq!(chunk.code, vec![OpCode::Nil, OpCode::Print, OpCode::Loop(2)]);
    assert_eq!(chunk.lines, vec![1, 3, 4]);
}

#[test]
fn test_e4c_17_multiple_jumps_targeting_same_eliminated_jump() {
    let mut chunk = rlox::chunk::Chunk::new();
    chunk.write(OpCode::Jump(2), 1);        // 0 -> apunta a 3 (0 + 1 + 2 = 3)
    chunk.write(OpCode::JumpIfFalse(1), 1); // 1 -> apunta a 3 (1 + 1 + 1 = 3)
    chunk.write(OpCode::Print, 2);          // 2 -> nuevo 2
    chunk.write(OpCode::Jump(0), 3);        // 3 -> eliminado
    chunk.write(OpCode::Return, 4);         // 4 -> nuevo 3

    compiler::simplify_chunk_jumps(&mut chunk);

    // En el chunk compactado:
    // 0: Jump(2) -> nuevo target 3 (Return): offset = 3 - (0 + 1) = 2
    // 1: JumpIfFalse(1) -> nuevo target 3 (Return): offset = 3 - (1 + 1) = 1
    // 2: Print
    // 3: Return
    assert_eq!(
        chunk.code,
        vec![
            OpCode::Jump(2),
            OpCode::JumpIfFalse(1),
            OpCode::Print,
            OpCode::Return
        ]
    );
    assert_eq!(chunk.lines, vec![1, 1, 2, 4]);
}

// =========================================================================
// Auditoría Final E4: Integración y Composición de Optimizaciones
// =========================================================================

#[test]
fn test_audit_e4_1_cli_flag_combinations() {
    // 1. Niveles estándar
    let o0 = OptConfig::from_level(OptLevel::O0);
    assert!(!o0.constant_folding);
    assert!(!o0.dead_code_elimination);
    assert!(!o0.jump_simplification);

    let o1 = OptConfig::from_level(OptLevel::O1);
    assert!(o1.constant_folding);
    assert!(o1.dead_code_elimination);
    assert!(o1.jump_simplification);
    assert!(!o1.peephole);

    let o2 = OptConfig::from_level(OptLevel::O2);
    assert!(o2.constant_folding);
    assert!(o2.dead_code_elimination);
    assert!(o2.jump_simplification);
    assert!(o2.peephole);

    // 2. Combinación múltiple separada por comas: -O0 --opt=fold,dce,jumps
    let mut o0_custom = OptConfig::from_level(OptLevel::O0);
    o0_custom.apply_flags("fold,dce,jumps", true).unwrap();
    assert!(o0_custom.constant_folding);
    assert!(o0_custom.dead_code_elimination);
    assert!(o0_custom.jump_simplification);
    assert!(!o0_custom.peephole);

    // 3. Desactivación múltiple: -O2 --no-opt=fold,dce,jumps
    let mut o2_custom = OptConfig::from_level(OptLevel::O2);
    o2_custom.apply_flags("fold,dce,jumps", false).unwrap();
    assert!(!o2_custom.constant_folding);
    assert!(!o2_custom.dead_code_elimination);
    assert!(!o2_custom.jump_simplification);
    assert!(o2_custom.peephole); // Las optimizaciones de O2 no afectadas permanecen
    assert!(o2_custom.superinstructions);

    // 4. Activación / desactivación individual
    let mut config = OptConfig::default();
    config.set_pass(OptPass::ConstantFolding, true);
    assert!(config.constant_folding && !config.dead_code_elimination);
    config.set_pass(OptPass::ConstantFolding, false);
    assert!(!config.constant_folding);
}

#[test]
fn test_audit_e4_2_folding_plus_dce_composition() {
    let test_cases = [
        // (código, salida esperada)
        ("if (10 > 5) print \"correcto\"; else print \"incorrecto\";", "correcto\n"),
        ("if (nil) print \"fallo\"; else print \"ok\";", "ok\n"),
        ("if (false) print \"fallo\"; else print \"ok\";", "ok\n"),
        ("if (0) print \"cero es truthy\"; else print \"fallo\";", "cero es truthy\n"),
        ("if (\"\") print \"vacio es truthy\"; else print \"fallo\";", "vacio es truthy\n"),
        ("if ((2 + 3) * 4 == 20) print \"aritmetica plegada\"; else print \"fallo\";", "aritmetica plegada\n"),
        ("if (-5 < 2) print \"negacion plegada\"; else print \"fallo\";", "negacion plegada\n"),
    ];

    for (src, expected) in test_cases {
        let (out_o0, code_o0) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
        let (out_o1, code_o1) = run_with_opt(src, OptConfig::from_level(OptLevel::O1));

        let mut o0_flags = OptConfig::from_level(OptLevel::O0);
        o0_flags.apply_flags("fold,dce", true).unwrap();
        let (out_flags, code_flags) = run_with_opt(src, o0_flags);

        assert_eq!(out_o0, expected, "O0 salida para: {}", src);
        assert_eq!(out_o1, expected, "O1 salida para: {}", src);
        assert_eq!(out_flags, expected, "Flags salida para: {}", src);

        // En O1 y fold+dce, no deben emitirse saltos condicionales en estos casos
        assert!(code_o0.iter().any(|op| matches!(op, OpCode::JumpIfFalse(_))));
        assert!(!code_o1.iter().any(|op| matches!(op, OpCode::JumpIfFalse(_))));
        assert_eq!(code_o1, code_flags);
    }
}

#[test]
fn test_audit_e4_3_dce_plus_jump_simplification() {
    // Comprueba anidamientos, ramas muertas y compactación
    let src = r#"
        var a = 1;
        if (false) {
            if (true) {
                print "muerto";
            }
        } else {
            if (a > 0) {
                print "vivo";
            }
        }
    "#;

    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let (out_o1, code_o1) = run_with_opt(src, OptConfig::from_level(OptLevel::O1));

    assert_eq!(out_o0, out_o1);
    assert_eq!(out_o1, "vivo\n");

    // Ningún Jump(0) debe sobrevivir en O1
    assert!(!code_o1.iter().any(|op| matches!(op, OpCode::Jump(0))));
}

#[test]
fn test_audit_e4_4_three_optimizations_full_composition() {
    let complex_src = r#"
        class Base {
            init(val) {
                this.val = val;
            }
            compute(x) {
                return this.val + x;
            }
        }

        class Sub < Base {
            init(val, extra) {
                super.init(val);
                this.extra = extra;
            }
            compute(x) {
                if (10 > 20) {
                    return -1;
                    print "muerto tras return";
                }
                var base_val = super.compute(x);
                if (x > 0 and this.extra > 0) {
                    var acc = 0;
                    var i = 0;
                    while (i < 3) {
                        acc = acc + i;
                        i = i + 1;
                    }
                    return base_val + acc + this.extra;
                }
                return base_val;
            }
        }

        fun makeMultiplier(factor) {
            fun mult(n) {
                return n * factor;
            }
            return mult;
        }

        var obj = Sub(100, 5);
        print obj.compute(10);

        var triple = makeMultiplier(3);
        print triple(7);
    "#;

    let configs = [
        OptConfig::from_level(OptLevel::O0),
        OptConfig::from_level(OptLevel::O1),
        OptConfig::from_level(OptLevel::O2),
        {
            let mut c = OptConfig::from_level(OptLevel::O0);
            c.apply_flags("fold,dce,jumps", true).unwrap();
            c
        },
        {
            let mut c = OptConfig::from_level(OptLevel::O2);
            c.apply_flags("fold,dce,jumps", false).unwrap();
            c
        },
    ];

    let mut expected_output = String::new();
    for (i, cfg) in configs.iter().enumerate() {
        let (out, _) = run_with_opt(complex_src, cfg.clone());
        if i == 0 {
            expected_output = out.clone();
            assert_eq!(expected_output, "118\n21\n");
        } else {
            assert_eq!(out, expected_output, "Paridad en configuración #{:?}", i);
        }
    }
}

#[test]
fn test_audit_e4_5_compile_validations_in_dead_branches() {
    let invalid_dead_programs = [
        // this fuera de clase dentro de rama muerta
        "if (false) { this; }",
        // super fuera de clase dentro de rama muerta
        "if (false) { super.foo(); }",
        // return en nivel superior dentro de rama muerta
        "if (false) { return 42; }",
        // declaración duplicada en el mismo scope local
        "fun test() { if (false) { var a = 1; var a = 2; } } test();",
    ];

    for src in invalid_dead_programs {
        let mut scanner = Scanner::new(src.to_string());
        let tokens = scanner.scan_tokens().expect("tokens validos");
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().expect("parse valido");

        let chk_errors = checker::check(&stmts);
        if !chk_errors.is_empty() {
            // El checker estático rechaza el programa para todos los niveles de optimización
            continue;
        }

        let res_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0));
        let res_o1 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O1));
        let res_o2 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O2));

        assert!(res_o0.is_err(), "O0 debe rechazar: {}", src);
        assert!(res_o1.is_err(), "O1 no debe omitir error en: {}", src);
        assert!(res_o2.is_err(), "O2 no debe omitir error en: {}", src);
    }
}

#[test]
fn test_audit_e4_6_bytecode_invariants_recursive_verifier() {
    fn verify_proto_invariants(proto: &rlox::closure::FnProto) {
        let code = &proto.chunk.code;
        let lines = &proto.chunk.lines;
        let len = code.len();

        // 1. Longitud de código y líneas perfectamente alineadas
        assert_eq!(code.len(), lines.len(), "code y lines deben coincidir en tamaño");

        // 2. Destinos de saltos válidos y direcciones
        for (i, op) in code.iter().enumerate() {
            match op {
                OpCode::Jump(offset) => {
                    let target = i + 1 + *offset as usize;
                    assert!(target <= len, "Jump({}) en índice {} excede longitud {}", offset, i, len);
                    assert!(target > i, "Jump no puede saltar hacia atrás");
                }
                OpCode::JumpIfFalse(offset) => {
                    let target = i + 1 + *offset as usize;
                    assert!(target <= len, "JumpIfFalse({}) en índice {} excede longitud {}", offset, i, len);
                    assert!(target > i, "JumpIfFalse no puede saltar hacia atrás");
                }
                OpCode::Loop(offset) => {
                    assert!(
                        (i + 1) >= *offset as usize,
                        "Loop({}) en índice {} causaría underflow",
                        offset,
                        i
                    );
                    let target = (i + 1) - *offset as usize;
                    assert!(target <= i + 1, "Loop no puede saltar hacia adelante");
                }
                OpCode::Constant(k) => {
                    assert!((*k as usize) < proto.chunk.constants.len(), "Constante {} fuera de rango", k);
                }
                OpCode::Closure(k) => {
                    assert!((*k as usize) < proto.chunk.constants.len(), "Proto de closure {} fuera de rango", k);
                }
                _ => {}
            }
        }

        // 3. Verificar recursivamente constantes
        for c in &proto.chunk.constants {
            if let rlox::value::Value::Proto(sub_p) = c {
                verify_proto_invariants(sub_p);
            }
        }
    }

    let src = r#"
        class A {
            foo() {
                var sum = 0;
                for (var i = 0; i < 5; i = i + 1) {
                    if (i == 2) sum = sum + 10;
                    else sum = sum + i;
                }
                return sum;
            }
        }
        fun bar() {
            fun f(x) { return x * 2; }
            return f(A().foo());
        }
        print bar();
    "#;

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let stmts = parse_and_check(src);
        let proto = compiler::compile_with_opt(&stmts, OptConfig::from_level(level)).unwrap();
        verify_proto_invariants(&proto);
    }
}

#[test]
fn test_audit_e4_7_cycle_detection_conservative_preservation() {
    let mut chunk = rlox::chunk::Chunk::new();
    // 0: Jump(1) -> 2
    // 1: Print
    // 2: Jump(1) -> 4
    // 3: Print
    // 4: Jump(65534) -> target = (4 + 1) + 65534, pero simulamos un ciclo con saltos encadenados
    // En resolve_jump_chain:
    // 0 -> 2 -> 0 (ciclo)
    // Cuando el salto en 2 apunta a 0, next_target (0) <= origin (0) o visited.contains(0).
    // Debe retornar None y dejar el Jump original en 0.
    chunk.write(OpCode::Jump(1), 1); // 0 -> 2
    chunk.write(OpCode::Print, 2);   // 1
    chunk.write(OpCode::Jump(0), 3); // 2 -> Jump(0) apunta a 3
    chunk.write(OpCode::Return, 4);  // 3

    compiler::simplify_chunk_jumps(&mut chunk);

    // Debe resolver Jump(0) y Jump(1) sin caer en bucle y compactando adecuadamente
    assert!(!chunk.code.iter().any(|op| matches!(op, OpCode::Jump(0))));
}

#[test]
fn test_audit_e4_8_aborted_compaction_leaves_chunk_intact() {
    let mut chunk = rlox::chunk::Chunk::new();
    chunk.write(OpCode::Print, 1);
    chunk.write(OpCode::Jump(0), 2);
    // Salto corrupto que desborda el chunk original
    chunk.write(OpCode::Jump(9999), 3);

    let original_code = chunk.code.clone();
    let original_lines = chunk.lines.clone();

    // compact_chunk_jumps debe detectar que old_target > old_len y abortar de forma segura
    compiler::simplify_chunk_jumps(&mut chunk);

    assert_eq!(chunk.code, original_code);
    assert_eq!(chunk.lines, original_lines);
}

// ============================================================================
// Pruebas E5.A.1: Invoke Optimization (PrepareInvoke + Invoke)
// ============================================================================

fn find_opcode<F>(proto: &rlox::closure::FnProto, predicate: &F) -> bool
where
    F: Fn(&OpCode) -> bool,
{
    if proto.chunk.code.iter().any(predicate) {
        return true;
    }
    for c in &proto.chunk.constants {
        if let rlox::value::Value::Proto(p) = c {
            if find_opcode(p, predicate) {
                return true;
            }
        }
    }
    false
}

#[test]
fn test_e5a1_bytecode_emission_and_flags() {
    let src = r#"
        class Greeter {
            say(msg) { print msg; }
        }
        var g = Greeter();
        g.say("hola");
    "#;
    let stmts = parse_and_check(src);

    // 1. Con -O0 (sin invoke): debe contener GetProperty y Call, NO PrepareInvoke ni Invoke
    let opt_o0 = OptConfig::from_level(OptLevel::O0);
    let proto_o0 = compiler::compile_with_opt(&stmts, opt_o0).unwrap();
    assert!(find_opcode(&proto_o0, &|op| matches!(op, OpCode::GetProperty(_))));
    assert!(find_opcode(&proto_o0, &|op| matches!(op, OpCode::Call(_))));
    assert!(!find_opcode(&proto_o0, &|op| matches!(op, OpCode::PrepareInvoke(_))));
    assert!(!find_opcode(&proto_o0, &|op| matches!(op, OpCode::Invoke(_))));

    // 2. Con -O0 --opt=invoke: debe contener PrepareInvoke e Invoke
    let mut opt_o0_invoke = OptConfig::from_level(OptLevel::O0);
    opt_o0_invoke.apply_flags("invoke", true).unwrap();
    let proto_o0_invoke = compiler::compile_with_opt(&stmts, opt_o0_invoke).unwrap();
    assert!(find_opcode(&proto_o0_invoke, &|op| matches!(op, OpCode::PrepareInvoke(_))));
    assert!(find_opcode(&proto_o0_invoke, &|op| matches!(op, OpCode::Invoke(_))));

    // 3. Con -O2: invoke está activo por defecto
    let opt_o2 = OptConfig::from_level(OptLevel::O2);
    let proto_o2 = compiler::compile_with_opt(&stmts, opt_o2).unwrap();
    assert!(find_opcode(&proto_o2, &|op| matches!(op, OpCode::PrepareInvoke(_))));
    assert!(find_opcode(&proto_o2, &|op| matches!(op, OpCode::Invoke(_))));

    // 4. Con -O2 --no-opt=invoke: invoke se desactiva
    let mut opt_o2_no_invoke = OptConfig::from_level(OptLevel::O2);
    opt_o2_no_invoke.apply_flags("invoke", false).unwrap();
    let proto_o2_no_invoke = compiler::compile_with_opt(&stmts, opt_o2_no_invoke).unwrap();
    assert!(!find_opcode(&proto_o2_no_invoke, &|op| matches!(op, OpCode::PrepareInvoke(_))));
    assert!(!find_opcode(&proto_o2_no_invoke, &|op| matches!(op, OpCode::Invoke(_))));
}

#[test]
fn test_e5a1_direct_method_calls_arguments_and_this() {
    let src = r#"
        class Calc {
            init(base) {
                this.base = base;
            }
            zero() {
                return this.base;
            }
            one(x) {
                return this.base + x;
            }
            multi(a, b, c) {
                return this.base + a * b - c;
            }
            mutate(inc) {
                this.base = this.base + inc;
                return this.base;
            }
        }

        var c = Calc(100);
        print c.zero();
        print c.one(5);
        print c.multi(2, 3, 4);
        print c.mutate(50);
        print c.zero();
    "#;

    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_invoke = OptConfig::from_level(OptLevel::O0);
    opt_invoke.apply_flags("invoke", true).unwrap();
    let (out_invoke, _) = run_with_opt(src, opt_invoke);
    let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));

    let expected = "100\n105\n102\n150\n150\n";
    assert_eq!(out_o0, expected);
    assert_eq!(out_invoke, expected);
    assert_eq!(out_o2, expected);
}

#[test]
fn test_e5a1_field_shadowing_and_mutation() {
    // 1. Un campo que contiene una función
    let src1 = r#"
        class Host {
            test() { return "metodo"; }
        }
        var h = Host();
        fun campoFunc() { return "campo"; }
        h.test = campoFunc;
        print h.test();
    "#;
    let (out_o0, _) = run_with_opt(src1, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src1, opt_inv.clone());
    assert_eq!(out_o0, "campo\n");
    assert_eq!(out_inv, "campo\n");

    // 2. Mutación del campo durante la evaluación del argumento
    let src2 = r#"
        class A {
            init() {
                fun f(x) { return x + 1; }
                this.run = f;
            }
        }
        var a = A();
        fun side_effect() {
            a.run = 999;
            return 10;
        }
        print a.run(side_effect());
    "#;
    let (out2_o0, _) = run_with_opt(src2, OptConfig::from_level(OptLevel::O0));
    let (out2_inv, _) = run_with_opt(src2, opt_inv);
    assert_eq!(out2_o0, "11\n");
    assert_eq!(out2_inv, "11\n");
}

#[test]
fn test_e5a1_error_precedence_and_evaluation_order() {
    // Caso A: receptor no es instancia con argumento que fallaría
    let src1 = r#"
        var obj = 123;
        obj.method(variableInexistente);
    "#;
    let stmts1 = parse_and_check(src1);
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();

    let proto1_o0 = compiler::compile_with_opt(&stmts1, OptConfig::from_level(OptLevel::O0)).unwrap();
    let proto1_inv = compiler::compile_with_opt(&stmts1, opt_inv.clone()).unwrap();

    let mut vm_o0 = Vm::with_output(Vec::new());
    let err_o0 = vm_o0.interpret(proto1_o0).unwrap_err();

    let mut vm_inv = Vm::with_output(Vec::new());
    let err_inv = vm_inv.interpret(proto1_inv).unwrap_err();

    assert!(err_o0.contains("Only instances have properties."));
    assert!(err_inv.contains("Only instances have properties."));

    // Caso B: propiedad no existe con argumento con efecto secundario / división por cero
    let src2 = r#"
        class Foo {}
        var f = Foo();
        f.inexistente(1 / 0);
    "#;
    let stmts2 = parse_and_check(src2);
    let proto2_o0 = compiler::compile_with_opt(&stmts2, OptConfig::from_level(OptLevel::O0)).unwrap();
    let proto2_inv = compiler::compile_with_opt(&stmts2, opt_inv).unwrap();

    let mut vm2_o0 = Vm::with_output(Vec::new());
    let err2_o0 = vm2_o0.interpret(proto2_o0).unwrap_err();

    let mut vm2_inv = Vm::with_output(Vec::new());
    let err2_inv = vm2_inv.interpret(proto2_inv).unwrap_err();

    assert!(err2_o0.contains("Undefined property 'inexistente'."));
    assert!(err2_inv.contains("Undefined property 'inexistente'."));
}

#[test]
fn test_e5a1_nested_calls_and_order() {
    let src = r#"
        class Adder {
            add(x, y) {
                return x + y;
            }
            dup(x) {
                return x * 2;
            }
        }
        var a = Adder();
        print a.add(a.dup(5), a.dup(10));
        print a.dup(a.dup(a.dup(2)));
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src, opt_inv);
    let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));

    let expected = "30\n16\n";
    assert_eq!(out_o0, expected);
    assert_eq!(out_inv, expected);
    assert_eq!(out_o2, expected);
}

#[test]
fn test_e5a1_recursive_methods_and_closures() {
    let src = r#"
        class Math {
            fact(n) {
                if (n <= 1) return 1;
                return n * this.fact(n - 1);
            }
            makeAdder(x) {
                fun adder(y) {
                    return x + y;
                }
                return adder;
            }
        }
        var m = Math();
        print m.fact(5);
        var add10 = m.makeAdder(10);
        print add10(25);
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src, opt_inv);
    let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));

    let expected = "120\n35\n";
    assert_eq!(out_o0, expected);
    assert_eq!(out_inv, expected);
    assert_eq!(out_o2, expected);
}

#[test]
fn test_e5a1_non_callable_field_error() {
    let src = r#"
        class Box {
            init() {
                this.val = 42;
            }
        }
        var b = Box();
        b.val();
    "#;
    let stmts = parse_and_check(src);
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();

    let proto_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0)).unwrap();
    let proto_inv = compiler::compile_with_opt(&stmts, opt_inv).unwrap();

    let mut vm_o0 = Vm::with_output(Vec::new());
    let err_o0 = vm_o0.interpret(proto_o0).unwrap_err();

    let mut vm_inv = Vm::with_output(Vec::new());
    let err_inv = vm_inv.interpret(proto_inv).unwrap_err();

    assert!(err_o0.contains("Can only call functions and classes."));
    assert!(err_inv.contains("Can only call functions and classes."));
}

#[test]
fn test_e5a1_inherited_method_resolution_three_levels() {
    let src = r#"
        class A {
            foo() { return "A_foo"; }
        }
        class B < A {}
        class C < B {}

        var c = C();
        print c.foo();
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, code_inv) = run_with_opt(src, opt_inv);
    let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));

    assert_eq!(out_o0, "A_foo\n");
    assert_eq!(out_inv, "A_foo\n");
    assert_eq!(out_o2, "A_foo\n");

    // Verificar que efectivamente se emitió PrepareInvoke e Invoke
    assert!(code_inv.iter().any(|op| matches!(op, OpCode::PrepareInvoke(_))));
    assert!(code_inv.iter().any(|op| matches!(op, OpCode::Invoke(_))));
}

#[test]
fn test_e5a1_prepared_calls_integrity_nested_recursion() {
    let src = r#"
        class A {
            method(x) { return x; }
        }

        fun nested(n) {
            if (n <= 0) return 1;
            var a = A();
            return a.method(nested(n - 1));
        }

        var a = A();
        print a.method(nested(10));
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src, opt_inv);
    let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));

    assert_eq!(out_o0, "1\n");
    assert_eq!(out_inv, "1\n");
    assert_eq!(out_o2, "1\n");
}

#[test]
fn test_e5a1_field_shadowing_various_callables() {
    // 1. Campo que contiene una función normal
    let src_func = r#"
        class H {}
        var h = H();
        fun f(x) { return x * 2; }
        h.m = f;
        print h.m(21);
    "#;
    let (out_f_o0, _) = run_with_opt(src_func, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_f_inv, _) = run_with_opt(src_func, opt_inv.clone());
    assert_eq!(out_f_o0, "42\n");
    assert_eq!(out_f_inv, "42\n");

    // 2. Campo que contiene una clase invocable
    let src_class = r#"
        class Target {
            init(val) { this.val = val; }
        }
        class H {}
        var h = H();
        h.m = Target;
        var t = h.m("creado");
        print t.val;
    "#;
    let (out_c_o0, _) = run_with_opt(src_class, OptConfig::from_level(OptLevel::O0));
    let (out_c_inv, _) = run_with_opt(src_class, opt_inv.clone());
    assert_eq!(out_c_o0, "creado\n");
    assert_eq!(out_c_inv, "creado\n");

    // 3. Campo que contiene un BoundMethod previo
    let src_bound = r#"
        class Base {
            say(greeting) { return greeting + " desde Base"; }
        }
        var b = Base();
        var bound = b.say;
        class H {}
        var h = H();
        h.m = bound;
        print h.m("hola");
    "#;
    let (out_b_o0, _) = run_with_opt(src_bound, OptConfig::from_level(OptLevel::O0));
    let (out_b_inv, _) = run_with_opt(src_bound, opt_inv.clone());
    assert_eq!(out_b_o0, "hola desde Base\n");
    assert_eq!(out_b_inv, "hola desde Base\n");

    // 4. Campo que contiene un valor no invocable (error idéntico)
    let src_err = r#"
        class H {}
        var h = H();
        h.m = 12345;
        h.m();
    "#;
    let stmts_err = parse_and_check(src_err);
    let proto_err_o0 = compiler::compile_with_opt(&stmts_err, OptConfig::from_level(OptLevel::O0)).unwrap();
    let proto_err_inv = compiler::compile_with_opt(&stmts_err, opt_inv.clone()).unwrap();

    let mut vm_err_o0 = Vm::with_output(Vec::new());
    let err_o0 = vm_err_o0.interpret(proto_err_o0).unwrap_err();

    let mut vm_err_inv = Vm::with_output(Vec::new());
    let err_inv = vm_err_inv.interpret(proto_err_inv).unwrap_err();

    assert!(err_o0.contains("Can only call functions and classes."));
    assert!(err_inv.contains("Can only call functions and classes."));

    // 5. Error de aridad en función asignada a campo
    let src_arity = r#"
        class H {}
        var h = H();
        fun two_args(a, b) { return a + b; }
        h.m = two_args;
        h.m(1);
    "#;
    let stmts_arity = parse_and_check(src_arity);
    let proto_ar_o0 = compiler::compile_with_opt(&stmts_arity, OptConfig::from_level(OptLevel::O0)).unwrap();
    let proto_ar_inv = compiler::compile_with_opt(&stmts_arity, opt_inv).unwrap();

    let mut vm_ar_o0 = Vm::with_output(Vec::new());
    let err_ar_o0 = vm_ar_o0.interpret(proto_ar_o0).unwrap_err();

    let mut vm_ar_inv = Vm::with_output(Vec::new());
    let err_ar_inv = vm_ar_inv.interpret(proto_ar_inv).unwrap_err();

    assert!(err_ar_o0.contains("Expected 2 arguments but got 1."));
    assert!(err_ar_inv.contains("Expected 2 arguments but got 1."));
}

// ============================================================================
// Pruebas E5.A.2: SuperInvoke Optimization (PrepareSuperInvoke + Invoke)
// ============================================================================

#[test]
fn test_e5a2_bytecode_emission_and_flags() {
    let src = r#"
        class A {
            foo() { return 1; }
        }
        class B < A {
            test() {
                super.foo();
            }
            extract() {
                var f = super.foo;
                return f;
            }
        }
    "#;
    let stmts = parse_and_check(src);

    // 1. Con -O0: no debe haber PrepareSuperInvoke
    let proto_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0)).unwrap();
    assert!(find_opcode(&proto_o0, &|op| matches!(op, OpCode::GetSuper(_))));
    assert!(!find_opcode(&proto_o0, &|op| matches!(op, OpCode::PrepareSuperInvoke(_))));

    // 2. Con -O0 --opt=invoke: llamada a super.foo() debe emitir PrepareSuperInvoke e Invoke
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let proto_inv = compiler::compile_with_opt(&stmts, opt_inv).unwrap();
    assert!(find_opcode(&proto_inv, &|op| matches!(op, OpCode::PrepareSuperInvoke(_))));
    assert!(find_opcode(&proto_inv, &|op| matches!(op, OpCode::Invoke(_))));
    // Pero extract() sigue usando GetSuper porque no es llamada directa
    assert!(find_opcode(&proto_inv, &|op| matches!(op, OpCode::GetSuper(_))));

    // 3. Con -O2: PrepareSuperInvoke activo
    let proto_o2 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O2)).unwrap();
    assert!(find_opcode(&proto_o2, &|op| matches!(op, OpCode::PrepareSuperInvoke(_))));

    // 4. Con -O2 --no-opt=invoke: se desactiva
    let mut opt_o2_no_inv = OptConfig::from_level(OptLevel::O2);
    opt_o2_no_inv.apply_flags("invoke", false).unwrap();
    let proto_o2_no_inv = compiler::compile_with_opt(&stmts, opt_o2_no_inv).unwrap();
    assert!(!find_opcode(&proto_o2_no_inv, &|op| matches!(op, OpCode::PrepareSuperInvoke(_))));
}

#[test]
fn test_e5a2_basic_and_multi_arguments_with_this_binding() {
    let src = r#"
        class A {
            init(x) {
                this.x = x;
            }
            calc(a, b, c) {
                this.x = this.x + a * b - c;
                return this.x;
            }
        }
        class B < A {
            test(a, b, c) {
                return super.calc(a, b, c);
            }
        }

        var b = B(100);
        print b.test(2, 5, 10);
        print b.x;
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src, opt_inv);
    let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));

    let expected = "100\n100\n"; // 100 + 2*5 - 10 = 100
    assert_eq!(out_o0, expected);
    assert_eq!(out_inv, expected);
    assert_eq!(out_o2, expected);
}

#[test]
fn test_e5a2_three_level_hierarchy_and_override() {
    let src = r#"
        class A {
            m() { return "A"; }
        }
        class B < A {
            m() { return "B -> " + super.m(); }
        }
        class C < B {
            m() { return "C -> " + super.m(); }
        }

        var c = C();
        print c.m();
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src, opt_inv);
    let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));

    let expected = "C -> B -> A\n";
    assert_eq!(out_o0, expected);
    assert_eq!(out_inv, expected);
    assert_eq!(out_o2, expected);
}

#[test]
fn test_e5a2_lexical_super_resolution_in_inherited_method() {
    let src = r#"
        class A {
            method() { return "A"; }
        }
        class B < A {
            method() { return "B"; }
            test() { return super.method(); }
        }
        class C < B {
            method() { return "C"; }
        }

        var c = C();
        print c.test();
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src, opt_inv);

    assert_eq!(out_o0, "A\n");
    assert_eq!(out_inv, "A\n");
}

#[test]
fn test_e5a2_undefined_method_and_error_precedence() {
    let src = r#"
        class A {}
        class B < A {
            test() {
                super.inexistente(1 / 0);
            }
        }
        B().test();
    "#;
    let stmts = parse_and_check(src);
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();

    let proto_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0)).unwrap();
    let proto_inv = compiler::compile_with_opt(&stmts, opt_inv).unwrap();

    let mut vm_o0 = Vm::with_output(Vec::new());
    let err_o0 = vm_o0.interpret(proto_o0).unwrap_err();

    let mut vm_inv = Vm::with_output(Vec::new());
    let err_inv = vm_inv.interpret(proto_inv).unwrap_err();

    assert!(err_o0.contains("Undefined property 'inexistente'."));
    assert!(err_inv.contains("Undefined property 'inexistente'."));
}

#[test]
fn test_e5a2_wrong_arity_error() {
    let src = r#"
        class A {
            m(x, y) { return x + y; }
        }
        class B < A {
            test() {
                super.m(10);
            }
        }
        B().test();
    "#;
    let stmts = parse_and_check(src);
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();

    let proto_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0)).unwrap();
    let proto_inv = compiler::compile_with_opt(&stmts, opt_inv).unwrap();

    let mut vm_o0 = Vm::with_output(Vec::new());
    let err_o0 = vm_o0.interpret(proto_o0).unwrap_err();

    let mut vm_inv = Vm::with_output(Vec::new());
    let err_inv = vm_inv.interpret(proto_inv).unwrap_err();

    assert!(err_o0.contains("Expected 2 arguments but got 1."));
    assert!(err_inv.contains("Expected 2 arguments but got 1."));
}

#[test]
fn test_e5a2_nested_prepare_invoke_and_prepare_super_invoke() {
    let src = r#"
        class A {
            add(x, y) { return x + y; }
        }
        class B < A {
            dup(x) { return x * 2; }
            calc() {
                return super.add(this.dup(10), super.add(2, 3));
            }
        }

        var b = B();
        print b.calc();
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src, opt_inv);
    let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));

    // this.dup(10) = 20, super.add(2, 3) = 5, super.add(20, 5) = 25
    assert_eq!(out_o0, "25\n");
    assert_eq!(out_inv, "25\n");
    assert_eq!(out_o2, "25\n");
}

#[test]
fn test_e5a2_recursion_with_super() {
    let src = r#"
        class A {
            fact(n) {
                if (n <= 1) return 1;
                return n * this.fact(n - 1);
            }
        }
        class B < A {
            fact(n) {
                return super.fact(n);
            }
        }

        var b = B();
        print b.fact(6);
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src, opt_inv);

    assert_eq!(out_o0, "720\n");
    assert_eq!(out_inv, "720\n");
}

#[test]
fn test_e5a2_closure_capturing_super() {
    let src = r#"
        class A {
            greeting() { return "hola desde A"; }
        }
        class B < A {
            makeClosure() {
                fun f() {
                    return super.greeting();
                }
                return f;
            }
        }

        var b = B();
        var clos = b.makeClosure();
        print clos();
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src, opt_inv);

    assert_eq!(out_o0, "hola desde A\n");
    assert_eq!(out_inv, "hola desde A\n");
}

#[test]
fn test_e5a2_extraction_without_call() {
    let src = r#"
        class A {
            greet() { return "metodo A"; }
        }
        class B < A {
            getGreeter() {
                return super.greet;
            }
        }

        var b = B();
        var fn_ref = b.getGreeter();
        print fn_ref();
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_inv = OptConfig::from_level(OptLevel::O0);
    opt_inv.apply_flags("invoke", true).unwrap();
    let (out_inv, _) = run_with_opt(src, opt_inv);

    assert_eq!(out_o0, "metodo A\n");
    assert_eq!(out_inv, "metodo A\n");
}

// ============================================================================
// Pruebas E5.B: Indexed Globals (Fast Path con Inline Caching)
// ============================================================================

#[test]
fn test_e5b_bytecode_emission_and_flags() {
    let src = "var x = 10; x = x + 1; print x;";
    let stmts = parse_and_check(src);

    // 1. -O0: GetGlobal y SetGlobal, NO GetGlobalFast ni SetGlobalFast
    let proto_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0)).unwrap();
    assert!(find_opcode(&proto_o0, &|op| matches!(op, OpCode::GetGlobal(_))));
    assert!(find_opcode(&proto_o0, &|op| matches!(op, OpCode::SetGlobal(_))));
    assert!(!find_opcode(&proto_o0, &|op| matches!(op, OpCode::GetGlobalFast(_))));
    assert!(!find_opcode(&proto_o0, &|op| matches!(op, OpCode::SetGlobalFast(_))));

    // 2. -O0 --opt=globals: GetGlobalFast y SetGlobalFast
    let mut opt_globals = OptConfig::from_level(OptLevel::O0);
    opt_globals.apply_flags("globals", true).unwrap();
    let proto_globals = compiler::compile_with_opt(&stmts, opt_globals).unwrap();
    assert!(find_opcode(&proto_globals, &|op| matches!(op, OpCode::GetGlobalFast(_))));
    assert!(find_opcode(&proto_globals, &|op| matches!(op, OpCode::SetGlobalFast(_))));

    // 3. -O2: GetGlobalFast activo por defecto
    let proto_o2 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O2)).unwrap();
    assert!(find_opcode(&proto_o2, &|op| matches!(op, OpCode::GetGlobalFast(_))));
    assert!(find_opcode(&proto_o2, &|op| matches!(op, OpCode::SetGlobalFast(_))));

    // 4. -O2 --no-opt=globals: revierte a GetGlobal y SetGlobal
    let mut opt_o2_no_globals = OptConfig::from_level(OptLevel::O2);
    opt_o2_no_globals.apply_flags("globals", false).unwrap();
    let proto_o2_no_globals = compiler::compile_with_opt(&stmts, opt_o2_no_globals).unwrap();
    assert!(find_opcode(&proto_o2_no_globals, &|op| matches!(op, OpCode::GetGlobal(_))));
    assert!(find_opcode(&proto_o2_no_globals, &|op| matches!(op, OpCode::SetGlobal(_))));
    assert!(!find_opcode(&proto_o2_no_globals, &|op| matches!(op, OpCode::GetGlobalFast(_))));
    assert!(!find_opcode(&proto_o2_no_globals, &|op| matches!(op, OpCode::SetGlobalFast(_))));
}

#[test]
fn test_e5b_late_binding_and_functions() {
    let src = r#"
        fun f() {
            return x;
        }

        var x = 10;
        print f();
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_globals = OptConfig::from_level(OptLevel::O0);
    opt_globals.apply_flags("globals", true).unwrap();
    let (out_globals, _) = run_with_opt(src, opt_globals);
    let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));

    assert_eq!(out_o0, "10\n");
    assert_eq!(out_globals, "10\n");
    assert_eq!(out_o2, "10\n");
}

#[test]
fn test_e5b_undefined_global_read_and_assign_errors() {
    // 1. Lectura de global inexistente
    let src_read = "print variableInexistente;";
    let stmts_read = parse_and_check(src_read);
    let mut opt_globals = OptConfig::from_level(OptLevel::O0);
    opt_globals.apply_flags("globals", true).unwrap();

    let proto_r_o0 = compiler::compile_with_opt(&stmts_read, OptConfig::from_level(OptLevel::O0)).unwrap();
    let proto_r_opt = compiler::compile_with_opt(&stmts_read, opt_globals.clone()).unwrap();

    let mut vm_r_o0 = Vm::with_output(Vec::new());
    let err_r_o0 = vm_r_o0.interpret(proto_r_o0).unwrap_err();

    let mut vm_r_opt = Vm::with_output(Vec::new());
    let err_r_opt = vm_r_opt.interpret(proto_r_opt).unwrap_err();

    assert_eq!(err_r_o0, "[line 1] Error: Undefined variable 'variableInexistente'.");
    assert_eq!(err_r_opt, "[line 1] Error: Undefined variable 'variableInexistente'.");

    // 2. Asignación a global inexistente
    let src_assign = "variableInexistente = 123;";
    let stmts_assign = parse_and_check(src_assign);

    let proto_a_o0 = compiler::compile_with_opt(&stmts_assign, OptConfig::from_level(OptLevel::O0)).unwrap();
    let proto_a_opt = compiler::compile_with_opt(&stmts_assign, opt_globals).unwrap();

    let mut vm_a_o0 = Vm::with_output(Vec::new());
    let err_a_o0 = vm_a_o0.interpret(proto_a_o0).unwrap_err();

    let mut vm_a_opt = Vm::with_output(Vec::new());
    let err_a_opt = vm_a_opt.interpret(proto_a_opt).unwrap_err();

    assert_eq!(err_a_o0, "[line 1] Error: Undefined variable 'variableInexistente'.");
    assert_eq!(err_a_opt, "[line 1] Error: Undefined variable 'variableInexistente'.");
}

#[test]
fn test_e5b_redefinition_top_level() {
    let src1 = r#"
        var x = 1;
        var x = 2;
        print x;
    "#;
    let (out1_o0, _) = run_with_opt(src1, OptConfig::from_level(OptLevel::O0));
    let mut opt_g = OptConfig::from_level(OptLevel::O0);
    opt_g.apply_flags("globals", true).unwrap();
    let (out1_g, _) = run_with_opt(src1, opt_g.clone());
    assert_eq!(out1_o0, "2\n");
    assert_eq!(out1_g, "2\n");

    let src2 = r#"
        var x = 1;
        var x = x + 10;
        print x;
    "#;
    let (out2_o0, _) = run_with_opt(src2, OptConfig::from_level(OptLevel::O0));
    let (out2_g, _) = run_with_opt(src2, opt_g);
    assert_eq!(out2_o0, "11\n");
    assert_eq!(out2_g, "11\n");
}

#[test]
fn test_e5b_closures_observing_mutated_globals() {
    let src = r#"
        var x = 1;
        fun f() {
            print x;
        }
        x = 2;
        f();
        x = 99;
        f();
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_g = OptConfig::from_level(OptLevel::O0);
    opt_g.apply_flags("globals", true).unwrap();
    let (out_g, _) = run_with_opt(src, opt_g);
    assert_eq!(out_o0, "2\n99\n");
    assert_eq!(out_g, "2\n99\n");
}

#[test]
fn test_e5b_multiple_interpret_calls_repl_continuity() {
    let mut opt_g = OptConfig::from_level(OptLevel::O0);
    opt_g.apply_flags("globals", true).unwrap();

    let stmts1 = parse_and_check("var a = 100;");
    let proto1 = compiler::compile_with_opt(&stmts1, opt_g.clone()).unwrap();

    let stmts2 = parse_and_check("var b = 200; print a + b;");
    let proto2 = compiler::compile_with_opt(&stmts2, opt_g.clone()).unwrap();

    let stmts3 = parse_and_check("a = a + 50; print a;");
    let proto3 = compiler::compile_with_opt(&stmts3, opt_g).unwrap();

    let mut buf = Vec::new();
    let mut vm = Vm::with_output(&mut buf);
    vm.interpret(proto1).unwrap();
    vm.interpret(proto2).unwrap();
    vm.interpret(proto3).unwrap();

    let output = String::from_utf8(buf).unwrap().replace("\r\n", "\n");
    assert_eq!(output, "300\n150\n");
}

#[test]
fn test_e5b_isolated_vm_instances() {
    let mut opt_g = OptConfig::from_level(OptLevel::O0);
    opt_g.apply_flags("globals", true).unwrap();

    let stmts = parse_and_check("print x;");
    let proto = compiler::compile_with_opt(&stmts, opt_g).unwrap();

    // VM 1 define x = "primera"
    let mut buf1 = Vec::new();
    let mut vm1 = Vm::with_output(&mut buf1);
    let stmts1 = parse_and_check("var x = \"primera\";");
    vm1.interpret(compiler::compile_with_opt(&stmts1, OptConfig::default()).unwrap()).unwrap();
    vm1.interpret(proto.clone()).unwrap();

    // VM 2 define x = "segunda"
    let mut buf2 = Vec::new();
    let mut vm2 = Vm::with_output(&mut buf2);
    let stmts2 = parse_and_check("var x = \"segunda\";");
    vm2.interpret(compiler::compile_with_opt(&stmts2, OptConfig::default()).unwrap()).unwrap();
    vm2.interpret(proto).unwrap();

    let out1 = String::from_utf8(buf1).unwrap().replace("\r\n", "\n");
    let out2 = String::from_utf8(buf2).unwrap().replace("\r\n", "\n");
    assert_eq!(out1, "primera\n");
    assert_eq!(out2, "segunda\n");
}

#[test]
fn test_e5b_composition_with_all_optimizations() {
    let src = r#"
        var i = 0;
        var acc = 0;
        while (i < 10) {
            acc = acc + i;
            i = i + 1;
        }
        print acc;
    "#;
    let (out_o0, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O0));
    let mut opt_g = OptConfig::from_level(OptLevel::O0);
    opt_g.apply_flags("globals", true).unwrap();
    let (out_g, _) = run_with_opt(src, opt_g);
    let (out_o2, _) = run_with_opt(src, OptConfig::from_level(OptLevel::O2));

    let mut opt_o2_no_g = OptConfig::from_level(OptLevel::O2);
    opt_o2_no_g.apply_flags("globals", false).unwrap();
    let (out_o2_no_g, _) = run_with_opt(src, opt_o2_no_g);

    let expected = "45\n";
    assert_eq!(out_o0, expected);
    assert_eq!(out_g, expected);
    assert_eq!(out_o2, expected);
    assert_eq!(out_o2_no_g, expected);
}
