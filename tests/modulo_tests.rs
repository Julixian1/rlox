//! Tests exhaustivos para la verificación del operador módulo (%) en rlox.
//! Cubre números positivos, negativos, decimales, precedencia, asociatividad,
//! casos borde con cero/NaN/infinito, operandos inválidos, expresiones complejas,
//! bucles y paridad entre Tree-Walk, Stack VM (-O0 y -O2) y Register VM.

use rlox::checker;
use rlox::chunk::OpCode;
use rlox::closure::FnProto;
use rlox::compiler::{
    self,
    opt::{OptConfig, OptLevel},
};
use rlox::interpreter::Interpreter;
use rlox::parser::Parser;
use rlox::resolver::Resolver;
use rlox::scanner::Scanner;
use rlox::stmt::Stmt;
use rlox::value::Value;
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

fn find_opcode<F>(proto: &FnProto, predicate: &F) -> bool
where
    F: Fn(&OpCode) -> bool,
{
    if proto.chunk.code.iter().any(predicate) {
        return true;
    }
    for c in &proto.chunk.constants {
        if let Value::Proto(p) = c {
            if find_opcode(p, predicate) {
                return true;
            }
        }
    }
    false
}

/// Helper para verificar paridad absoluta entre los 4 backends:
/// Tree-Walk, Stack VM (-O0), Stack VM (-O2) y Register VM.
fn assert_all_backends_ok(source: &str, expected_stdout: &str) {
    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
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
    assert_eq!(vm_o2.stdout, expected_stdout, "VM -O2 stdout no coincide");
    assert_eq!(
        reg.stdout, expected_stdout,
        "Register VM stdout no coincide"
    );
}

/// Helper para verificar que un error de tipo en tiempo de ejecución
/// sea emitido equivalentemente en los 4 backends con número de línea y mensaje esperado.
fn assert_all_backends_runtime_error(source: &str, expected_line: usize, expected_msg: &str) {
    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);
    let reg = run_register_vm(source);

    assert!(tw.error.is_some(), "TreeWalk debio fallar");
    assert!(vm_o0.error.is_some(), "VM -O0 debio fallar");
    assert!(vm_o2.error.is_some(), "VM -O2 debio fallar");
    assert!(reg.error.is_some(), "Register VM debio fallar");

    let expected_pattern = format!("[line {}] {}", expected_line, expected_msg);
    assert!(
        tw.error.as_ref().unwrap().contains(&expected_pattern),
        "TreeWalk error inesperado: {:?}",
        tw.error
    );
    assert!(
        vm_o0.error.as_ref().unwrap().contains(&expected_pattern),
        "VM -O0 error inesperado: {:?}",
        vm_o0.error
    );
    assert!(
        vm_o2.error.as_ref().unwrap().contains(&expected_pattern),
        "VM -O2 error inesperado: {:?}",
        vm_o2.error
    );
    assert!(
        reg.error.as_ref().unwrap().contains(&expected_pattern),
        "Register VM error inesperado: {:?}",
        reg.error
    );
}

#[test]
fn test_modulo_positive_integers() {
    let source = "
    print 10 % 5;
    print 12 % 4;
    print 7 % 7;

    print 10 % 3;
    print 17 % 5;
    print 25 % 7;

    print 3 % 10;
    print 1 % 5;

    var a = 37;
    var b = 6;
    print a % b;
    ";
    let expected = "0\n0\n0\n1\n2\n4\n3\n1\n1\n";
    assert_all_backends_ok(source, expected);
}

#[test]
fn test_modulo_negative_operands() {
    let source = "
    // En semantica IEEE 754 / Rust f64 %, el signo del resto coincide con el dividendo
    print -10 % 3;
    print -17 % 5;

    print 10 % -3;
    print 17 % -5;

    print -10 % -3;
    print -17 % -5;

    var x = -25;
    var y = 7;
    print x % y;
    print -x % -y;
    ";
    let expected = "-1\n-2\n1\n2\n-1\n-2\n-4\n4\n";
    assert_all_backends_ok(source, expected);
}

#[test]
fn test_modulo_floating_point() {
    let source = "
    print 10.5 % 3;
    print 10.5 % 3.5;
    print -10.5 % 3;
    print 10.5 % -3;
    print -10.5 % -3;
    print 0.5 % 0.2;
    ";
    let expected = "1.5\n0\n-1.5\n1.5\n-1.5\n0.09999999999999998\n";
    assert_all_backends_ok(source, expected);
}

#[test]
fn test_modulo_precedence_with_arithmetic() {
    let source = "
    print 10 + 20 % 6;
    print 20 % 6 + 10;
    print 10 - 20 % 6;
    print 20 % 6 - 1;

    print 10 * 7 % 4;
    print 10 % 7 * 4;
    print 10 / 2 % 3;
    print 20 % 6 / 2;

    print (10 + 20) % 7;
    print 10 % (2 + 1);
    print (10 * 7) % (2 + 2);
    ";
    let expected = "12\n12\n8\n1\n2\n12\n2\n1\n2\n1\n2\n";
    assert_all_backends_ok(source, expected);
}

#[test]
fn test_modulo_left_associativity() {
    let source = "
    print 100 % 30 % 7;

    print 50 % 16 % 5 % 2;

    var x = 100;
    var y = 30;
    var z = 7;
    print x % y % z;
    ";
    let expected = "3\n0\n3\n";
    assert_all_backends_ok(source, expected);
}

#[test]
fn test_modulo_zero_nan_infinity() {
    let source = "
    // Modulo por cero produce NaN en IEEE 754
    print 10 % 0;
    print -10 % 0;
    print 0 % 0;

    print 0 % 5;
    print 0 % -5;

    var inf = 1 / 0;
    var nan = 0 / 0;

    print 10 % inf;
    print -10 % inf;

    print inf % 5;

    print nan % 5;
    print 5 % nan;
    print nan % nan;
    ";
    let expected = "NaN\nNaN\nNaN\n0\n0\n10\n-10\nNaN\nNaN\nNaN\nNaN\n";
    assert_all_backends_ok(source, expected);
}

#[test]
fn test_modulo_invalid_operand_string_left() {
    let source = "
    var s = \"texto\";
    print s % 2;
    ";
    assert_all_backends_runtime_error(source, 3, "Operands must be numbers.");
}

#[test]
fn test_modulo_invalid_operand_string_right() {
    let source = "
    var n = 10;
    print n % \"cadena\";
    ";
    assert_all_backends_runtime_error(source, 3, "Operands must be numbers.");
}

#[test]
fn test_modulo_invalid_operand_bool() {
    let source = "
    var b = true;
    print 10 % b;
    ";
    assert_all_backends_runtime_error(source, 3, "Operands must be numbers.");
}

#[test]
fn test_modulo_invalid_operand_nil() {
    let source = "
    var x = nil;
    print x % 3;
    ";
    assert_all_backends_runtime_error(source, 3, "Operands must be numbers.");
}

#[test]
fn test_modulo_in_loops_and_conditionals() {
    let source = "
    var evens = 0;
    var odds = 0;
    for (var i = 1; i <= 10; i = i + 1) {
        if (i % 2 == 0) {
            evens = evens + 1;
        } else {
            odds = odds + 1;
        }
    }
    print evens;
    print odds;

    fun mcd(a, b) {
        while (b != 0) {
            var temp = b;
            b = a % b;
            a = temp;
        }
        return a;
    }
    print mcd(48, 18);
    print mcd(100, 35);
    print mcd(7, 13);
    ";
    let expected = "5\n5\n6\n5\n1\n";
    assert_all_backends_ok(source, expected);
}

#[test]
fn test_modulo_constant_folding_bytecode_and_parity() {
    let source = "
    print 25 % 7;
    print -19 % 4;
    print 10.5 % 3;
    print 10 % 0;
    ";

    let stmts = parse_and_check(source).unwrap();

    // En -O0: NO debe haber constant folding para %, debe emitirse OpCode::Modulo
    let proto_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0)).unwrap();
    assert!(
        find_opcode(&proto_o0, &|op| matches!(op, OpCode::Modulo)),
        "-O0 debio emitir OpCode::Modulo"
    );

    // En -O1 y -O2: Debe realizar Constant Folding para las operaciones con literales
    let proto_o1 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O1)).unwrap();
    let proto_o2 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O2)).unwrap();

    assert!(
        !find_opcode(&proto_o1, &|op| matches!(op, OpCode::Modulo)),
        "-O1 debio plegar todas las operaciones de modulo constante"
    );
    assert!(
        !find_opcode(&proto_o2, &|op| matches!(op, OpCode::Modulo)),
        "-O2 debio plegar todas las operaciones de modulo constante"
    );

    let res_o0 = run_stack_vm(source, OptLevel::O0);
    let res_o1 = run_stack_vm(source, OptLevel::O1);
    let res_o2 = run_stack_vm(source, OptLevel::O2);

    assert_eq!(res_o0.stdout, res_o1.stdout);
    assert_eq!(res_o1.stdout, res_o2.stdout);
    assert_eq!(res_o0.stdout, "4\n-3\n1.5\nNaN\n");
}
