//! Tests exhaustivos para el operador condicional ternario (`? :`) en rlox.
//!
//! Cubre:
//! - Condiciones verdaderas y falsas con tipos diversos.
//! - Truthiness en Lox: nil y false son falsy; 0, "", strings, numeros y objetos son truthy.
//! - Resultados de distintos tipos (numeros, strings, booleanos, nil).
//! - Ternarios anidados en la rama else (asociatividad a derecha) y en la rama then.
//! - Precedencia con operadores logicos (or, and) y aritmeticos/comparaciones.
//! - Precedencia e interaccion con asignaciones simples (=) y compuestas (+=, etc.).
//! - Evaluacion unica de la condicion.
//! - Evaluacion condicional estricta (short-circuit): la rama no seleccionada no produce
//!   efectos secundarios ni errores en tiempo de ejecucion.
//! - Recursion, funciones y bucles utilizando el operador ternario.
//! - Errores sintacticos en expresiones incompletas.
//! - Paridad entre Tree-Walk, Stack VM (-O0, -O1, -O2) y Register VM.

use rlox::checker;
use rlox::compiler::{
    self,
    opt::{OptConfig, OptLevel},
};
use rlox::interpreter::Interpreter;
use rlox::parser::Parser;
use rlox::resolver::Resolver;
use rlox::scanner::Scanner;
use rlox::stmt::Stmt;
use rlox::vm::Vm;

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct SharedOutput(Arc<Mutex<Vec<u8>>>);

impl Write for SharedOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RunResult {
    stdout: String,
    error: Option<String>,
}

fn parse_and_check(source: &str) -> Result<Vec<Stmt>, String> {
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens()?;
    let mut parser = Parser::new(tokens);
    let statements = parser.parse()?;
    let errors = checker::check(&statements);
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(statements)
}

fn run_treewalk(source: &str) -> RunResult {
    let statements = match parse_and_check(source) {
        Ok(s) => s,
        Err(e) => {
            return RunResult {
                stdout: String::new(),
                error: Some(e),
            };
        }
    };

    let mut resolver = Resolver::new();
    resolver.resolve_stmts(&statements);

    let output_arc = Arc::new(Mutex::new(Vec::new()));
    let mut interpreter = Interpreter::with_output(SharedOutput(Arc::clone(&output_arc)));

    for (ptr, depth) in resolver.locals {
        interpreter.resolve(ptr, depth);
    }

    let err = interpreter.interpret(&statements).err();
    let stdout = {
        let bytes = output_arc.lock().unwrap().clone();
        String::from_utf8(bytes)
            .unwrap_or_default()
            .replace("\r\n", "\n")
    };

    RunResult { stdout, error: err }
}

fn run_stack_vm(source: &str, opt_level: OptLevel) -> RunResult {
    let statements = match parse_and_check(source) {
        Ok(s) => s,
        Err(e) => {
            return RunResult {
                stdout: String::new(),
                error: Some(e),
            };
        }
    };

    let opt_config = OptConfig::from_level(opt_level);
    let proto = match compiler::compile_with_opt(&statements, opt_config) {
        Ok(p) => p,
        Err(e) => {
            return RunResult {
                stdout: String::new(),
                error: Some(e),
            };
        }
    };

    let mut output = Vec::new();
    let err = {
        let mut vm = Vm::with_output(&mut output);
        vm.interpret(proto).err()
    };
    let stdout = String::from_utf8(output)
        .unwrap_or_default()
        .replace("\r\n", "\n");

    RunResult { stdout, error: err }
}

fn run_register_vm(source: &str) -> RunResult {
    let statements = match parse_and_check(source) {
        Ok(s) => s,
        Err(e) => {
            return RunResult {
                stdout: String::new(),
                error: Some(e),
            };
        }
    };

    let proto = match rlox::compiler::reg::codegen::compile(&statements) {
        Ok(p) => p,
        Err(e) => {
            return RunResult {
                stdout: String::new(),
                error: Some(e),
            };
        }
    };

    let mut output = Vec::new();
    let err = {
        let mut vm = rlox::compiler::reg::vm::RegVm::with_output(&mut output);
        vm.interpret(proto).err()
    };
    let stdout = String::from_utf8(output)
        .unwrap_or_default()
        .replace("\r\n", "\n");

    RunResult { stdout, error: err }
}

/// Helper para verificar paridad absoluta entre los 5 modos:
/// TreeWalk, Stack VM (-O0), Stack VM (-O1), Stack VM (-O2) y Register VM.
fn assert_all_backends_ok(source: &str, expected_stdout: &str) {
    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o1 = run_stack_vm(source, OptLevel::O1);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);
    let reg = run_register_vm(source);

    assert!(
        tw.error.is_none(),
        "TreeWalk fallo inesperadamente: {:?}",
        tw.error
    );
    assert!(
        vm_o0.error.is_none(),
        "VM -O0 fallo inesperadamente: {:?}",
        vm_o0.error
    );
    assert!(
        vm_o1.error.is_none(),
        "VM -O1 fallo inesperadamente: {:?}",
        vm_o1.error
    );
    assert!(
        vm_o2.error.is_none(),
        "VM -O2 fallo inesperadamente: {:?}",
        vm_o2.error
    );
    assert!(
        reg.error.is_none(),
        "Register VM fallo inesperadamente: {:?}",
        reg.error
    );

    assert_eq!(tw.stdout, expected_stdout, "TreeWalk stdout no coincide");
    assert_eq!(vm_o0.stdout, expected_stdout, "VM -O0 stdout no coincide");
    assert_eq!(vm_o1.stdout, expected_stdout, "VM -O1 stdout no coincide");
    assert_eq!(vm_o2.stdout, expected_stdout, "VM -O2 stdout no coincide");
    assert_eq!(
        reg.stdout, expected_stdout,
        "Register VM stdout no coincide"
    );
}

#[test]
fn test_ternary_basic_true_false() {
    let source = r#"
        var t = true ? "verdadero" : "falso";
        print t;

        var f = false ? "verdadero" : "falso";
        print f;
    "#;
    assert_all_backends_ok(source, "verdadero\nfalso\n");
}

#[test]
fn test_ternary_truthiness_rules() {
    let source = r#"
        print nil ? "si" : "no";

        print false ? "si" : "no";

        print 0 ? "cero truthy" : "fallo";

        print "" ? "vacio truthy" : "fallo";

        print "hola" ? "texto truthy" : "fallo";
        print 42 ? "numero truthy" : "fallo";
    "#;
    assert_all_backends_ok(
        source,
        "no\nno\ncero truthy\nvacio truthy\ntexto truthy\nnumero truthy\n",
    );
}

#[test]
fn test_ternary_various_return_types() {
    let source = r#"
        print true ? 123 : 456;
        print false ? "a" : "b";
        print true ? nil : "no-nil";
        print false ? false : true;
    "#;
    assert_all_backends_ok(source, "123\nb\nnil\ntrue\n");
}

#[test]
fn test_ternary_short_circuit_side_effects() {
    let source = r#"
        var contador = 0;

        fun incrementar() {
            contador += 1;
            return contador;
        }

        print true ? incrementar() : incrementar();
        print contador;

        print false ? incrementar() : incrementar();
        print contador;
    "#;
    assert_all_backends_ok(source, "1\n1\n2\n2\n");
}

#[test]
fn test_ternary_short_circuit_avoids_runtime_error_in_unselected_branch() {
    let source = r#"
        // En la rama falsa habria un error de tipo ("hola" - 5), pero no debe evaluarse
        var res1 = true ? 100 : ("hola" - 5);
        print res1;

        // En la rama verdadera habria un error de tipo (true * 2), pero no debe evaluarse
        var res2 = false ? (true * 2) : 200;
        print res2;
    "#;
    assert_all_backends_ok(source, "100\n200\n");
}

#[test]
fn test_ternary_single_condition_evaluation() {
    let source = r#"
        var condCalls = 0;

        fun check(val) {
            condCalls += 1;
            return val;
        }

        var res = check(true) ? "si" : "no";
        print res;
        print condCalls;
    "#;
    assert_all_backends_ok(source, "si\n1\n");
}

#[test]
fn test_ternary_right_associativity() {
    let source = r#"
        var resultado = false ? 1 : true ? 2 : 3;
        print resultado;

        var r2 = false ? 1 : false ? 2 : 3;
        print r2;

        var r3 = false ? 1 : false ? 2 : true ? 4 : 5;
        print r3;
    "#;
    assert_all_backends_ok(source, "2\n3\n4\n");
}

#[test]
fn test_ternary_nested_in_then_branch() {
    let source = r#"
        var a = true ? (false ? "t-t" : "t-f") : "f";
        print a;

        var b = true ? false ? 1 : 2 : 3;
        print b;
    "#;
    assert_all_backends_ok(source, "t-f\n2\n");
}

#[test]
fn test_ternary_precedence_with_arithmetic_and_comparisons() {
    let source = r#"
        var x = 10;
        var r = x > 5 ? x * 2 : x + 2;
        print r;

        var edad = 20;
        print edad >= 18 ? "Mayor" : "Menor";

        print 2 + 3 == 5 ? "suma ok" : "suma mal";

        print true ? 10 + 5 * 2 : 0;
    "#;
    assert_all_backends_ok(source, "20\nMayor\nsuma ok\n20\n");
}

#[test]
fn test_ternary_precedence_with_logical_operators() {
    let source = r#"
        print false or true ? 10 : 20;

        print true and false ? 10 : 20;

        print true ? false or true : false and true;
    "#;
    assert_all_backends_ok(source, "10\n20\ntrue\n");
}

#[test]
fn test_ternary_interaction_with_assignments() {
    let source = r#"
        var a = 0;
        var b = 0;

        a = true ? 10 : 20;
        print a;

        a += false ? 10 : 5;
        print a;

        true ? (b = 5) : (b = 10);
        print b;

        false ? (b = 100) : (b = 50);
        print b;

        true ? (b += 20) : (b += 200);
        print b;
    "#;
    assert_all_backends_ok(source, "10\n15\n5\n50\n70\n");
}

#[test]
fn test_ternary_in_recursion_factorial_and_fibonacci() {
    let source = r#"
        fun factorial(n) {
            return n <= 1 ? 1 : n * factorial(n - 1);
        }

        print factorial(0);
        print factorial(1);
        print factorial(5);

        fun fib(n) {
            return n < 2 ? n : fib(n - 1) + fib(n - 2);
        }

        print fib(0);
        print fib(1);
        print fib(7);
    "#;
    assert_all_backends_ok(source, "1\n1\n120\n0\n1\n13\n");
}

#[test]
fn test_ternary_in_loops() {
    let source = r#"
        var countEven = 0;
        var countOdd = 0;

        for (var i = 1; i <= 6; i += 1) {
            i % 2 == 0 ? (countEven += 1) : (countOdd += 1);
        }

        print countEven;
        print countOdd;
    "#;
    assert_all_backends_ok(source, "3\n3\n");
}

#[test]
fn test_ternary_in_closures() {
    let source = r#"
        fun makeClassifier(threshold) {
            fun classify(val) {
                return val >= threshold ? "alto" : "bajo";
            }
            return classify;
        }

        var c = makeClassifier(10);
        print c(15);
        print c(5);
    "#;
    assert_all_backends_ok(source, "alto\nbajo\n");
}

#[test]
fn test_ternary_syntax_error_missing_colon() {
    let source = "true ? 1;";
    let res = parse_and_check(source);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("Expected ':' after then branch of conditional expression."),
        "Error inesperado: {}",
        err
    );
}

#[test]
fn test_ternary_syntax_error_missing_then_expression() {
    let source = "true ? : 2;";
    let res = parse_and_check(source);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("Expected expression."),
        "Error inesperado: {}",
        err
    );
}

#[test]
fn test_ternary_syntax_error_missing_else_expression() {
    let source = "true ? 1 :;";
    let res = parse_and_check(source);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("Expected expression."),
        "Error inesperado: {}",
        err
    );
}

#[test]
fn test_ternary_syntax_error_missing_condition() {
    let source = "? 1 : 2;";
    let res = parse_and_check(source);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("Expected expression."),
        "Error inesperado: {}",
        err
    );
}

#[test]
fn test_ternary_branch_assignments_and_expression_values() {
    let source = r#"
        var a = 0;
        var b = 0;

        print true ? (a = 5) : (b = 10);
        print a;
        print b;

        print false ? (a += 5) : (b += 10);
        print a;
        print b;

        var x = 10;
        x += true ? 5 : 2;
        print x;
    "#;
    assert_all_backends_ok(source, "5\n5\n0\n10\n5\n10\n15\n");
}

#[test]
fn test_ternary_unparenthesized_assignment_in_branch_rejected() {
    let source = "true ? a = 5 : b = 10;";
    let res = parse_and_check(source);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("Invalid assignment target."),
        "Error inesperado: {}",
        err
    );
}
