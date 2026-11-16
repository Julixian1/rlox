//! Tests exhaustivos para listas nativas en rlox.
//!
//! Cubre:
//! - Creación de listas vacías y con múltiples elementos.
//! - Valores heterogéneos y anidamiento multidimensional.
//! - Lectura y escritura por índice (con retorno del valor asignado).
//! - Indexación encadenada (`matriz[0][1]`).
//! - Aliasing y mutabilidad compartida por referencia.
//! - Paso de listas como argumentos y listas retornadas por funciones.
//! - Listas capturadas por closures.
//! - Funciones nativas `len` y `push` (validación de aridad y tipos, retorno nil en push).
//! - Errores en tiempo de ejecución: índices negativos, no enteros, fuera de rango y receptores inválidos.
//! - Asignaciones compuestas sobre índices (`+=`, `*=`, etc.) con evaluación única de receptor e índice.
//! - Igualdad por identidad de referencia entre listas (`==`, `!=`).
//! - Paridad absoluta entre Tree-Walk, Stack VM (-O0, -O1, -O2) y Register VM.

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

fn assert_all_backends_ok(source: &str, expected_stdout: &str) {
    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o1 = run_stack_vm(source, OptLevel::O1);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);
    let reg = run_register_vm(source);

    assert!(
        tw.error.is_none(),
        "TreeWalk falló inesperadamente: {:?}",
        tw.error
    );
    assert!(
        vm_o0.error.is_none(),
        "VM -O0 falló inesperadamente: {:?}",
        vm_o0.error
    );
    assert!(
        vm_o1.error.is_none(),
        "VM -O1 falló inesperadamente: {:?}",
        vm_o1.error
    );
    assert!(
        vm_o2.error.is_none(),
        "VM -O2 falló inesperadamente: {:?}",
        vm_o2.error
    );
    assert!(
        reg.error.is_none(),
        "RegVM falló inesperadamente: {:?}",
        reg.error
    );

    assert_eq!(tw.stdout, expected_stdout, "TreeWalk stdout difiere");
    assert_eq!(vm_o0.stdout, expected_stdout, "VM -O0 stdout difiere");
    assert_eq!(vm_o1.stdout, expected_stdout, "VM -O1 stdout difiere");
    assert_eq!(vm_o2.stdout, expected_stdout, "VM -O2 stdout difiere");
    assert_eq!(reg.stdout, expected_stdout, "RegVM stdout difiere");
}

fn assert_all_backends_runtime_error(source: &str, expected_substr: &str) {
    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o1 = run_stack_vm(source, OptLevel::O1);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);
    let reg = run_register_vm(source);

    assert!(
        tw.error.is_some(),
        "TreeWalk debía fallar pero retornó Ok: {}",
        tw.stdout
    );
    assert!(vm_o0.error.is_some(), "VM -O0 debía fallar pero retornó Ok");
    assert!(vm_o1.error.is_some(), "VM -O1 debía fallar pero retornó Ok");
    assert!(vm_o2.error.is_some(), "VM -O2 debía fallar pero retornó Ok");
    assert!(reg.error.is_some(), "RegVM debía fallar pero retornó Ok");

    let err_tw = tw.error.unwrap();
    let err_o0 = vm_o0.error.unwrap();
    let err_o1 = vm_o1.error.unwrap();
    let err_o2 = vm_o2.error.unwrap();
    let err_reg = reg.error.unwrap();

    assert!(
        err_tw.contains(expected_substr),
        "TreeWalk error '{err_tw}' no contiene '{expected_substr}'"
    );
    assert!(
        err_o0.contains(expected_substr),
        "VM -O0 error '{err_o0}' no contiene '{expected_substr}'"
    );
    assert!(
        err_o1.contains(expected_substr),
        "VM -O1 error '{err_o1}' no contiene '{expected_substr}'"
    );
    assert!(
        err_o2.contains(expected_substr),
        "VM -O2 error '{err_o2}' no contiene '{expected_substr}'"
    );
    assert!(
        err_reg.contains(expected_substr),
        "RegVM error '{err_reg}' no contiene '{expected_substr}'"
    );
}

#[test]
fn test_list_literals_and_display() {
    let src = r#"
var vacia = [];
var numeros = [1, 2, 3];
var mixta = [1, "hola", true, nil];
print vacia;
print numeros;
print mixta;
"#;
    assert_all_backends_ok(src, "[]\n[1, 2, 3]\n[1, hola, true, nil]\n");
}

#[test]
fn test_list_read_and_write_by_index() {
    let src = r#"
var a = [10, 20, 30];
print a[0];
print a[2];

var r = (a[1] = 99);
print r;
print a[1];
"#;
    assert_all_backends_ok(src, "10\n30\n99\n99\n");
}

#[test]
fn test_list_nested_and_chained_indexing() {
    let src = r#"
var matriz = [[1, 2], [3, 4]];
print matriz[0][1];
print matriz[1][0];

matriz[1][0] = 99;
print matriz[1][0];
print matriz;
"#;
    assert_all_backends_ok(src, "2\n3\n99\n[[1, 2], [99, 4]]\n");
}

#[test]
fn test_list_aliasing_shared_mutability() {
    let src = r#"
var a = [1, 2, 3];
var b = a;

b[0] = 99;
print a[0];
print b[0];
"#;
    assert_all_backends_ok(src, "99\n99\n");
}

#[test]
fn test_list_identity_equality() {
    let src = r#"
var a = [1, 2];
var b = [1, 2];
var c = a;

print a == b; // false (identidad de referencia)
print a != b; // true
print a == c; // true
print a != c; // false
"#;
    assert_all_backends_ok(src, "false\ntrue\ntrue\nfalse\n");
}

#[test]
fn test_list_as_argument_and_return_value() {
    let src = r#"
fun modificar(lista, idx, val) {
    lista[idx] = val;
}

fun crear(x, y) {
    return [x, y];
}

var l = crear(10, 20);
modificar(l, 0, 50);
print l[0];
print l[1];
"#;
    assert_all_backends_ok(src, "50\n20\n");
}

#[test]
fn test_list_in_closures() {
    let src = r#"
fun make_collector() {
    var items = [];
    fun add(val) {
        push(items, val);
        return items;
    }
    return add;
}

var c = make_collector();
c(1);
c(2);
var res = c(3);
print len(res);
print res[0];
print res[2];
"#;
    assert_all_backends_ok(src, "3\n1\n3\n");
}

#[test]
fn test_native_len_and_push() {
    let src = r#"
var a = [10, 20];
print len(a);

var r = push(a, 30);
print r;
print len(a);
print a[2];

push(a, 40);
print len(a);
print a[3];
"#;
    assert_all_backends_ok(src, "2\nnil\n3\n30\n4\n40\n");
}

#[test]
fn test_list_compound_assignments() {
    let src = r#"
var a = [10, 20, 30];
a[0] += 5;
a[1] *= 2;
a[2] %= 7;

print a[0];
print a[1];
print a[2];
"#;
    assert_all_backends_ok(src, "15\n40\n2\n");
}

#[test]
fn test_list_compound_assignment_single_evaluation() {
    let src = r#"
var contador_idx = 0;
fun get_idx() {
    contador_idx += 1;
    return 0;
}

var contador_obj = 0;
fun get_list() {
    contador_obj += 1;
    return [10];
}

var a = [10];
a[get_idx()] += 5;
print contador_idx; // 1
print a[0];         // 15

var l = get_list();
print contador_obj; // 1
"#;
    assert_all_backends_ok(src, "1\n15\n1\n");
}

#[test]
fn test_list_in_loops_and_conditionals() {
    let src = r#"
var lista = [];
for (var i = 0; i < 5; i += 1) {
    if (i == 2) continue;
    push(lista, i);
}

print len(lista);
for (var j = 0; j < len(lista); j += 1) {
    print lista[j];
}
"#;
    assert_all_backends_ok(src, "4\n0\n1\n3\n4\n");
}

#[test]
fn test_list_runtime_independence_not_shared_constant() {
    let src = r#"
fun get_new_list() {
    return [0];
}

var l1 = get_new_list();
var l2 = get_new_list();
l1[0] = 42;

print l1[0];
print l2[0];
"#;
    assert_all_backends_ok(src, "42\n0\n");
}

#[test]
fn test_error_index_negative() {
    let src = "var a = [1, 2]; print a[-1];";
    assert_all_backends_runtime_error(src, "Index cannot be negative.");
}

#[test]
fn test_error_index_decimal() {
    let src = "var a = [1, 2]; print a[1.5];";
    assert_all_backends_runtime_error(src, "Index must be an integer.");
}

#[test]
fn test_error_index_out_of_bounds_read() {
    let src = "var a = [1, 2]; print a[2];";
    assert_all_backends_runtime_error(src, "Index out of bounds");
}

#[test]
fn test_error_index_out_of_bounds_write() {
    let src = "var a = [1, 2]; a[5] = 99;";
    assert_all_backends_runtime_error(src, "Index out of bounds");
}

#[test]
fn test_error_index_non_number() {
    let src = "var a = [1, 2]; print a[\"0\"];";
    assert_all_backends_runtime_error(src, "Index must be a number.");
}

#[test]
fn test_error_index_non_list_read() {
    let src = "var a = 123; print a[0];";
    assert_all_backends_runtime_error(src, "Only lists can be indexed.");
}

#[test]
fn test_error_index_non_list_write() {
    let src = "var a = nil; a[0] = 5;";
    assert_all_backends_runtime_error(src, "Only lists can be indexed.");
}

#[test]
fn test_error_len_wrong_arity() {
    let src = "len();";
    assert_all_backends_runtime_error(src, "Expected 1 arguments but got 0.");
}

#[test]
fn test_error_len_not_a_list() {
    let src = "len(123);";
    assert_all_backends_runtime_error(src, "Argument to 'len' must be a list.");
}

#[test]
fn test_error_push_wrong_arity() {
    let src = "push([1]);";
    assert_all_backends_runtime_error(src, "Expected 2 arguments but got 1.");
}

#[test]
fn test_error_push_not_a_list() {
    let src = "push(\"hola\", 5);";
    assert_all_backends_runtime_error(src, "First argument to 'push' must be a list.");
}

#[test]
fn test_list_borrow_non_conflict_in_expressions() {
    let src = r#"
var a = [10, 20];
push(a, a[0]);
print a;

var b = [1, 2];
b[0] = b[1] + 5;
print b;
"#;
    assert_all_backends_ok(src, "[10, 20, 10]\n[7, 2]\n");
}

#[test]
fn test_list_receiver_and_index_single_evaluation() {
    let src = r#"
var llamadas = 0;
var a = [10];

fun obtenerLista() {
  llamadas += 1;
  return a;
}

fun obtenerIndice() {
  llamadas += 1;
  return 0;
}

obtenerLista()[obtenerIndice()] += 5;

print llamadas;
print a[0];
"#;
    assert_all_backends_ok(src, "2\n15\n");
}

#[test]
fn test_list_cyclic_reference_creation() {
    let src = r#"
var a = [];
push(a, a);
print len(a);
print a == a[0];
print a;
"#;
    assert_all_backends_ok(src, "1\ntrue\n[[...]]\n");
}

#[test]
fn test_list_cyclic_indirect_print() {
    let src = r#"
var a = [];
var b = [a];
push(a, b);
print a;
"#;
    assert_all_backends_ok(src, "[[[...]]]\n");
}

#[test]
fn test_list_shared_references_no_cycle_print() {
    let src = r#"
var a = [1, 2];
var b = [a, a];
print b;
"#;
    assert_all_backends_ok(src, "[[1, 2], [1, 2]]\n");
}

#[test]
fn test_native_pop_basic() {
    let src = r#"
var a = [10, 20, 30];
print pop(a);
print pop(a);
print len(a);
print a[0];
"#;
    assert_all_backends_ok(src, "30\n20\n1\n10\n");
}

#[test]
fn test_native_pop_aliasing() {
    let src = r#"
var a = [1, 2];
var b = a;
print pop(b);
print len(a);
print a[0];
"#;
    assert_all_backends_ok(src, "2\n1\n1\n");
}

#[test]
fn test_native_pop_nested_lists() {
    let src = r#"
var a = [[1, 2], [3, 4]];
var ultimo = pop(a);
print ultimo[0];
print ultimo[1];
print len(a);
"#;
    assert_all_backends_ok(src, "3\n4\n1\n");
}

#[test]
fn test_error_pop_empty_list() {
    let src = "var a = []; pop(a);";
    assert_all_backends_runtime_error(src, "Cannot pop from an empty list.");
}

#[test]
fn test_error_pop_wrong_arity() {
    let src = "pop();";
    assert_all_backends_runtime_error(src, "Expected 1 arguments but got 0.");
    let src2 = "pop([1], [2]);";
    assert_all_backends_runtime_error(src2, "Expected 1 arguments but got 2.");
}

#[test]
fn test_error_pop_not_a_list() {
    let src = "pop(\"hola\");";
    assert_all_backends_runtime_error(src, "Argument to 'pop' must be a list.");
}

#[test]
fn test_error_index_nan() {
    let src = "var a = [10, 20]; var nan = 0 / 0; print a[nan];";
    assert_all_backends_runtime_error(src, "Index must be an integer.");
}

#[test]
fn test_error_index_infinity() {
    let src = "var a = [10, 20]; var inf = 1 / 0; print a[inf];";
    assert_all_backends_runtime_error(src, "Index must be an integer.");
}

#[test]
fn test_error_index_negative_infinity() {
    let src = "var a = [10, 20]; var ninf = -1 / 0; print a[ninf];";
    assert_all_backends_runtime_error(src, "Index must be an integer.");
}

#[test]
fn test_error_index_set_nan() {
    let src = "var a = [10, 20]; var nan = 0 / 0; a[nan] = 5;";
    assert_all_backends_runtime_error(src, "Index must be an integer.");
}

#[test]
fn test_list_storing_and_calling_functions() {
    let src = r#"
fun add(a, b) {
  return a + b;
}
var ops = [add];
print ops[0](10, 20);
"#;
    assert_all_backends_ok(src, "30\n");
}

#[test]
fn test_list_recursion_and_closure_mutation() {
    let src = r#"
fun make_builder() {
  var items = [];
  fun fill(n) {
    if (n <= 0) return items;
    push(items, n);
    return fill(n - 1);
  }
  return fill;
}
var b = make_builder();
var res = b(3);
print len(res);
print res[0];
print res[1];
print res[2];
"#;
    assert_all_backends_ok(src, "3\n3\n2\n1\n");
}
