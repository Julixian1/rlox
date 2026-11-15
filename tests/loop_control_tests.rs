//! Tests exhaustivos para las sentencias `break` y `continue` en rlox.
//!
//! Cubre:
//! - `break` en bucles `while` y `for`.
//! - `continue` en bucles `while` y `for`.
//! - Semantica fundamental de `for`: `continue` ejecuta el incremento antes de reevaluar la condicion.
//! - Variantes de `for` con inicializador, condicion o incremento omitidos (incluyendo `for (;;)`).
//! - Bucles anidados: `break` y `continue` afectan unicamente al bucle mas interno.
//! - Limpieza de scopes y balance de stack: liberacion correcta de variables locales al salir de bloques.
//! - Cierre de upvalues (`CloseUpvalue` en Stack VM y `Close` en Register VM) cuando una variable
//!   capturada por closure es abandonada mediante `break` o `continue`.
//! - Validaciones estaticas del Checker: rechazo de `break` y `continue` fuera de bucles y dentro de
//!   funciones definidas dentro de bucles.
//! - Errores de sintaxis al omitir `;`.
//! - Paridad absoluta entre Tree-Walk, Stack VM (-O0, -O1, -O2) y Register VM.

use rlox::checker;
use rlox::compiler::{self, opt::{OptConfig, OptLevel}};
use rlox::interpreter::Interpreter;
use rlox::parser::Parser;
use rlox::resolver::Resolver;
use rlox::scanner::Scanner;
use rlox::stmt::Stmt;
use rlox::vm::Vm;

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Infraestructura de ejecucion y captura de salida
// ---------------------------------------------------------------------------

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
        Err(e) => return RunResult { stdout: String::new(), error: Some(e) },
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
        String::from_utf8(bytes).unwrap_or_default().replace("\r\n", "\n")
    };

    RunResult { stdout, error: err }
}

fn run_stack_vm(source: &str, opt_level: OptLevel) -> RunResult {
    let statements = match parse_and_check(source) {
        Ok(s) => s,
        Err(e) => return RunResult { stdout: String::new(), error: Some(e) },
    };

    let opt_config = OptConfig::from_level(opt_level);
    let proto = match compiler::compile_with_opt(&statements, opt_config) {
        Ok(p) => p,
        Err(e) => return RunResult { stdout: String::new(), error: Some(e) },
    };

    let mut output = Vec::new();
    let err = {
        let mut vm = Vm::with_output(&mut output);
        vm.interpret(proto).err()
    };
    let stdout = String::from_utf8(output).unwrap_or_default().replace("\r\n", "\n");

    RunResult { stdout, error: err }
}

fn run_register_vm(source: &str) -> RunResult {
    let statements = match parse_and_check(source) {
        Ok(s) => s,
        Err(e) => return RunResult { stdout: String::new(), error: Some(e) },
    };

    let proto = match rlox::compiler::reg::codegen::compile(&statements) {
        Ok(p) => p,
        Err(e) => return RunResult { stdout: String::new(), error: Some(e) },
    };

    let mut output = Vec::new();
    let err = {
        let mut vm = rlox::compiler::reg::vm::RegVm::with_output(&mut output);
        vm.interpret(proto).err()
    };
    let stdout = String::from_utf8(output).unwrap_or_default().replace("\r\n", "\n");

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

    assert!(tw.error.is_none(), "TreeWalk fallo inesperadamente: {:?}", tw.error);
    assert!(vm_o0.error.is_none(), "VM -O0 fallo inesperadamente: {:?}", vm_o0.error);
    assert!(vm_o1.error.is_none(), "VM -O1 fallo inesperadamente: {:?}", vm_o1.error);
    assert!(vm_o2.error.is_none(), "VM -O2 fallo inesperadamente: {:?}", vm_o2.error);
    assert!(reg.error.is_none(), "RegVM fallo inesperadamente: {:?}", reg.error);

    assert_eq!(tw.stdout, expected_stdout, "TreeWalk stdout difiere");
    assert_eq!(vm_o0.stdout, expected_stdout, "VM -O0 stdout difiere");
    assert_eq!(vm_o1.stdout, expected_stdout, "VM -O1 stdout difiere");
    assert_eq!(vm_o2.stdout, expected_stdout, "VM -O2 stdout difiere");
    assert_eq!(reg.stdout, expected_stdout, "RegVM stdout difiere");
}

// ---------------------------------------------------------------------------
// Tests de ejecucion y semantica de break / continue
// ---------------------------------------------------------------------------

#[test]
fn test_break_in_while() {
    let src = r#"
var i = 0;
while (i < 10) {
    if (i == 3) {
        break;
    }
    print i;
    i = i + 1;
}
print "done";
"#;
    assert_all_backends_ok(src, "0\n1\n2\ndone\n");
}

#[test]
fn test_continue_in_while() {
    let src = r#"
var i = 0;
while (i < 5) {
    i = i + 1;
    if (i == 3) {
        continue;
    }
    print i;
}
"#;
    assert_all_backends_ok(src, "1\n2\n4\n5\n");
}

#[test]
fn test_break_in_for() {
    let src = r#"
for (var i = 0; i < 10; i = i + 1) {
    if (i == 4) {
        break;
    }
    print i;
}
print "after";
"#;
    assert_all_backends_ok(src, "0\n1\n2\n3\nafter\n");
}

#[test]
fn test_continue_in_for_executes_increment() {
    // CRITICAL: continue en un for debe ejecutar el incremento antes de chequear la condicion.
    // Si saltara al inicio del bucle sin pasar por el incremento, caeria en bucle infinito o mantendria el mismo i.
    let src = r#"
for (var i = 0; i < 6; i = i + 1) {
    if (i == 2 or i == 4) {
        continue;
    }
    print i;
}
"#;
    assert_all_backends_ok(src, "0\n1\n3\n5\n");
}

#[test]
fn test_break_and_continue_combined_motivating_example() {
    let src = r#"
var i = 0;
while (i < 10) {
    i += 1;
    if (i == 3) continue;
    if (i == 7) break;
    print i;
}
"#;
    assert_all_backends_ok(src, "1\n2\n4\n5\n6\n");
}

#[test]
fn test_for_omitted_clauses() {
    // 1. Sin inicializador
    let src1 = r#"
var i = 0;
for (; i < 3; i = i + 1) {
    print i;
}
"#;
    assert_all_backends_ok(src1, "0\n1\n2\n");

    // 2. Sin condicion
    let src2 = r#"
for (var j = 0; ; j = j + 1) {
    if (j == 3) break;
    print j;
}
"#;
    assert_all_backends_ok(src2, "0\n1\n2\n");

    // 3. Sin incremento
    let src3 = r#"
for (var k = 0; k < 3;) {
    print k;
    k = k + 1;
}
"#;
    assert_all_backends_ok(src3, "0\n1\n2\n");

    // 4. Sin nada: for (;;)
    let src4 = r#"
var count = 0;
for (;;) {
    count = count + 1;
    if (count == 3) {
        break;
    }
}
print count;
"#;
    assert_all_backends_ok(src4, "3\n");
}

#[test]
fn test_nested_loops_break_innermost_only() {
    let src = r#"
for (var i = 0; i < 3; i = i + 1) {
    for (var j = 0; j < 3; j = j + 1) {
        if (j == 1) break;
        print i * 10 + j;
    }
}
"#;
    assert_all_backends_ok(src, "0\n10\n20\n");
}

#[test]
fn test_nested_loops_continue_innermost_only() {
    let src = r#"
for (var i = 0; i < 2; i = i + 1) {
    for (var j = 0; j < 3; j = j + 1) {
        if (j == 1) continue;
        print i * 10 + j;
    }
}
"#;
    assert_all_backends_ok(src, "0\n2\n10\n12\n");
}

#[test]
fn test_break_inside_deep_blocks_stack_balance() {
    // Al hacer break desde bloques anidados, todas las variables locales deben desapilarse correctamente.
    let src = r#"
var i = 0;
while (i < 5) {
    i = i + 1;
    var a = "a";
    {
        var b = "b";
        {
            var c = "c";
            if (i == 2) {
                break;
            }
        }
    }
}
var final_var = "ok";
print final_var;
"#;
    assert_all_backends_ok(src, "ok\n");
}

#[test]
fn test_continue_inside_deep_blocks_stack_balance() {
    // Al hacer continue desde bloques anidados en cada iteracion, el stack no debe crecer indefinidamente.
    let src = r#"
var total = 0;
for (var i = 0; i < 5; i = i + 1) {
    var a = 1;
    {
        var b = 2;
        {
            var c = 3;
            if (i >= 2) {
                continue;
            }
            total = total + a + b + c;
        }
    }
}
print total;
"#;
    // Para i=0: 1+2+3=6. Para i=1: 6+6=12. Para i=2,3,4: continue
    assert_all_backends_ok(src, "12\n");
}

#[test]
fn test_break_closes_upvalues() {
    // Comprueba que variables locales capturadas en closure se cierren correctamente (CloseUpvalue/Close)
    // al abandonar el bloque por break.
    let src = r#"
var fn_holder;
for (var i = 0; i < 5; i = i + 1) {
    var captured = "valor_" + "secreto";
    fun get_val() {
        return captured;
    }
    fn_holder = get_val;
    if (i == 1) {
        break;
    }
}
print fn_holder();
"#;
    assert_all_backends_ok(src, "valor_secreto\n");
}

#[test]
fn test_continue_closes_upvalues() {
    // Similarmente, si una variable local capturada es abandonada con continue,
    // el upvalue debe cerrarse y no quedar apuntando al stack reutilizado.
    let src = r#"
var fn1;
var fn2;
for (var i = 0; i < 3; i = i + 1) {
    var v = i * 10;
    fun read_v() { return v; }
    if (i == 0) {
        fn1 = read_v;
        continue;
    }
    if (i == 1) {
        fn2 = read_v;
        continue;
    }
}
print fn1();
print fn2();
"#;
    assert_all_backends_ok(src, "0\n10\n");
}

#[test]
fn test_break_for_loop_cleans_loop_variable() {
    // Verifica que la variable del for (i) se limpie apropiadamente al hacer break.
    let src = r#"
for (var i = 0; i < 100; i = i + 1) {
    if (i == 2) break;
}
var post = 42;
print post;
"#;
    assert_all_backends_ok(src, "42\n");
}

#[test]
fn test_break_in_while_with_compound_assignments() {
    let src = r#"
var x = 100;
while (x > 0) {
    x -= 15;
    if (x <= 50) {
        break;
    }
}
print x;
"#;
    // 100 -> 85 -> 70 -> 55 -> 40 (<=50 => break)
    assert_all_backends_ok(src, "40\n");
}

#[test]
fn test_continue_in_for_with_compound_assignments() {
    let src = r#"
var acc = 0;
for (var i = 1; i <= 5; i += 1) {
    if (i % 2 == 0) {
        continue;
    }
    acc += i;
}
print acc;
"#;
    // i=1: acc=1; i=2: cont; i=3: acc=4; i=4: cont; i=5: acc=9
    assert_all_backends_ok(src, "9\n");
}

#[test]
fn test_break_and_continue_with_ternary_condition() {
    let src = r#"
for (var i = 0; i < 10; i += 1) {
    var action = i < 3 ? "skip" : (i == 6 ? "stop" : "print");
    if (action == "skip") continue;
    if (action == "stop") break;
    print i;
}
"#;
    assert_all_backends_ok(src, "3\n4\n5\n");
}

#[test]
fn test_dead_code_elimination_preserves_break_and_continue() {
    // Comprueba que DCE (-O2) valide y no rompa bucles con break o continue incondicionales
    let src = r#"
var count = 0;
while (true) {
    count += 1;
    break;
    print "unreachable";
}
print count;
"#;
    assert_all_backends_ok(src, "1\n");
}

// ---------------------------------------------------------------------------
// Tests de rechazo en Checker y Errores Sintacticos
// ---------------------------------------------------------------------------

#[test]
fn test_checker_error_break_outside_loop() {
    let src = "break;";
    let res = parse_and_check(src);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("Error at 'break': Can't use 'break' outside of a loop."));
}

#[test]
fn test_checker_error_continue_outside_loop() {
    let src = "continue;";
    let res = parse_and_check(src);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("Error at 'continue': Can't use 'continue' outside of a loop."));
}

#[test]
fn test_checker_error_break_inside_block_outside_loop() {
    let src = "{ var x = 1; break; }";
    let res = parse_and_check(src);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("Error at 'break': Can't use 'break' outside of a loop."));
}

#[test]
fn test_checker_error_break_in_function_inside_loop() {
    // Una funcion declarada dentro de un bucle no puede contener break para salir del bucle exterior
    let src = r#"
while (true) {
    fun escape() {
        break;
    }
    escape();
}
"#;
    let res = parse_and_check(src);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("Error at 'break': Can't use 'break' outside of a loop."));
}

#[test]
fn test_checker_error_continue_in_function_inside_loop() {
    let src = r#"
for (var i = 0; i < 5; i += 1) {
    fun skip() {
        continue;
    }
    skip();
}
"#;
    let res = parse_and_check(src);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("Error at 'continue': Can't use 'continue' outside of a loop."));
}

#[test]
fn test_syntax_error_break_missing_semicolon() {
    let src = "while (true) { break }";
    let mut scanner = Scanner::new(src.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let res = parser.parse();
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("Expected ';' after 'break'."));
}

#[test]
fn test_syntax_error_continue_missing_semicolon() {
    let src = "while (true) { continue }";
    let mut scanner = Scanner::new(src.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let res = parser.parse();
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("Expected ';' after 'continue'."));
}

#[test]
fn test_closures_and_continue_in_for() {
    let src = r#"
var primero;
var segundo;

for (var i = 0; i < 2; i += 1) {
  {
    var x = i;

    fun capturar() {
      return x;
    }

    if (i == 0) {
      primero = capturar;
      continue;
    }

    segundo = capturar;
  }
}

print primero();
print segundo();
"#;
    assert_all_backends_ok(src, "0\n1\n");
}

#[test]
fn test_nested_loops_break_continue_optimizations() {
    let src = r#"
var total = 0;

for (var i = 0; i < 3; i += 1) {
  for (var j = 0; j < 5; j += 1) {
    if (j == 1) continue;
    if (j == 3) break;

    total += 1;
  }
}

print total;
"#;
    assert_all_backends_ok(src, "6\n");
}


