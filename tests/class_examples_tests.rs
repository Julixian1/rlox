//! Tests de integración para ejemplos consolidados de clases, métodos, herencia y 'super' en rlox.
//! Valida compatibilidad y paridad entre Tree-Walk, Stack VM (-O0 y -O2), errores semánticos/runtime
//! y documenta las limitaciones de Register VM.

use rlox::checker;
use rlox::chunk::OpCode;
use rlox::closure::FnProto;
use rlox::compiler::{self, opt::{OptConfig, OptLevel}};
use rlox::interpreter::Interpreter;
use rlox::parser::Parser;
use rlox::resolver::Resolver;
use rlox::scanner::Scanner;
use rlox::stmt::Stmt;
use rlox::value::Value;
use rlox::vm::Vm;

use std::fs;
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

/// Helper para verificar un archivo de ejemplo consolidado contra Tree-Walk, VM -O0 y VM -O2,
/// y validar el rechazo formal en Register VM (que no soporta clases).
fn assert_consolidated_example_ok(file_path: &str, expected_stdout: &str, feature_markers: &[&str]) {
    let source = fs::read_to_string(file_path)
        .unwrap_or_else(|_| panic!("No se pudo leer el archivo {}", file_path));

    let tw = run_treewalk(&source);
    assert!(
        tw.error.is_none(),
        "[{}] TreeWalk fallo inesperadamente: {:?}",
        file_path,
        tw.error
    );
    assert_eq!(
        tw.stdout, expected_stdout,
        "[{}] Salida inesperada en TreeWalk",
        file_path
    );

    let vm_o0 = run_stack_vm(&source, OptLevel::O0);
    assert!(
        vm_o0.error.is_none(),
        "[{}] Stack VM -O0 fallo inesperadamente: {:?}",
        file_path,
        vm_o0.error
    );
    assert_eq!(
        vm_o0.stdout, expected_stdout,
        "[{}] Salida inesperada en Stack VM -O0",
        file_path
    );

    let vm_o2 = run_stack_vm(&source, OptLevel::O2);
    assert!(
        vm_o2.error.is_none(),
        "[{}] Stack VM -O2 fallo inesperadamente: {:?}",
        file_path,
        vm_o2.error
    );
    assert_eq!(
        vm_o2.stdout, expected_stdout,
        "[{}] Salida inesperada en Stack VM -O2",
        file_path
    );

    assert_eq!(
        tw.stdout, vm_o0.stdout,
        "[{}] Diferencia entre TreeWalk y VM -O0",
        file_path
    );
    assert_eq!(
        vm_o0.stdout, vm_o2.stdout,
        "[{}] Diferencia entre VM -O0 y VM -O2",
        file_path
    );

    for marker in feature_markers {
        assert!(
            tw.stdout.contains(marker),
            "[{}] Salida no contiene el marcador de funcionalidad esperado: '{}'",
            file_path,
            marker
        );
    }

    let reg = run_register_vm(&source);
    assert!(
        reg.error.is_some(),
        "[{}] Register VM debio fallar por falta de soporte de clases pero tuvo exito",
        file_path
    );
    let reg_err = reg.error.unwrap();
    assert!(
        reg_err.contains("Classes are not supported yet in the register backend.")
            || reg_err.contains("Properties are not supported yet in the register backend.")
            || reg_err.contains("'this' and 'super' are not supported yet in the register backend."),
        "[{}] Error de Register VM no contiene mensaje esperado: {}",
        file_path,
        reg_err
    );
}

#[test]
fn test_example_basic() {
    let expected = "\
--- Propiedades iniciales ---
3
4
10
20
--- Estado independiente tras mutar p1.x ---
4
10
--- Suma de coordenadas ---
38
";
    let markers = [
        "--- Propiedades iniciales ---",
        "--- Estado independiente tras mutar p1.x ---",
        "--- Suma de coordenadas ---",
    ];
    assert_consolidated_example_ok("examples/classes/basic.lox", expected, &markers);
}

#[test]
fn test_example_methods() {
    let expected = "\
Dimensiones:
5
10
Area:
50
Perimetro:
30
--- Despues de escalar x2 ---
Dimensiones:
10
20
Area:
200
Perimetro:
60
--- Cuenta bancaria y retorno temprano en init ---
invalido
activo
Fondos insuficientes para Cuenta A
120
Cuenta A Renovada
500
";
    let markers = [
        "Dimensiones:\n5\n10",
        "Area:\n50",
        "Perimetro:\n30",
        "--- Despues de escalar x2 ---",
        "Area:\n200",
        "--- Cuenta bancaria y retorno temprano en init ---",
        "invalido",
        "Fondos insuficientes para Cuenta A",
        "Cuenta A Renovada\n500",
    ];
    assert_consolidated_example_ok("examples/classes/methods.lox", expected, &markers);
}

#[test]
fn test_example_inheritance() {
    let expected = "\
Firulais respira.
Firulais ladra: guau!
Gorrion vuela libremente.
Pingu respira.
Pingu no puede volar, pero nada habilmente.
--- Herencia multinivel ---
Tesla
Automovil electrico Tesla
Bateria electrica
";
    let markers = [
        "Firulais respira.",
        "Firulais ladra: guau!",
        "Gorrion vuela libremente.",
        "Pingu respira.",
        "Pingu no puede volar, pero nada habilmente.",
        "--- Herencia multinivel ---",
        "Automovil electrico Tesla",
    ];
    assert_consolidated_example_ok("examples/classes/inheritance.lox", expected, &markers);
}

#[test]
fn test_example_super() {
    let expected = "\
--- Render con llamadas encadenadas a super ---
### [PRIMARY] Widget(42) ###
--- ID preservado por super.init ---
42
--- Despacho dinamico con super ---
Beta delegando en -> Instancia concreta Beta
";
    let markers = [
        "--- Render con llamadas encadenadas a super ---",
        "### [PRIMARY] Widget(42) ###",
        "--- ID preservado por super.init ---",
        "42",
        "--- Despacho dinamico con super ---",
        "Beta delegando en -> Instancia concreta Beta",
    ];
    assert_consolidated_example_ok("examples/classes/super.lox", expected, &markers);
}

#[test]
fn test_example_polymorphism() {
    let expected = "\
--- Despacho polimorfico ---
Circulo -> area:
12.56636
Cuadrado -> area:
25
--- Metodo ligado invocado independientemente ---
16
--- Closure capturando 'this' ---
110
135
135
";
    let markers = [
        "--- Despacho polimorfico ---",
        "Circulo -> area:\n12.56636",
        "Cuadrado -> area:\n25",
        "--- Metodo ligado invocado independientemente ---",
        "16",
        "--- Closure capturando 'this' ---",
        "110",
        "135",
    ];
    assert_consolidated_example_ok("examples/classes/polymorphism.lox", expected, &markers);
}

#[test]
fn test_example_advanced() {
    let expected = "\
--- Metodo original ---
comportamiento por defecto del metodo
--- Metodo ocultado por funcion en campo ---
comportamiento personalizado inyectado en campo
--- Campo como valor escalar ---
propiedad escalar ahora
--- Evaluacion de argumentos (izq a der) ---
Evaluando argumento: primero
Evaluando argumento: segundo
Evaluando argumento: tercero
Parametros recibidos en metodo:
1
2
3
";
    let markers = [
        "--- Metodo original ---",
        "comportamiento por defecto del metodo",
        "--- Metodo ocultado por funcion en campo ---",
        "comportamiento personalizado inyectado en campo",
        "--- Campo como valor escalar ---",
        "propiedad escalar ahora",
        "--- Evaluacion de argumentos (izq a der) ---",
        "Evaluando argumento: primero\nEvaluando argumento: segundo\nEvaluando argumento: tercero",
    ];
    assert_consolidated_example_ok("examples/classes/advanced.lox", expected, &markers);
}

#[test]
fn test_error_property_on_non_instance() {
    let source = "
var not_an_instance = \"soy un string\";
print not_an_instance.campo;
";

    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);

    assert!(tw.error.is_some());
    assert!(vm_o0.error.is_some());
    assert!(vm_o2.error.is_some());

    let expected_msg = "[line 3] Only instances have properties.";
    assert!(tw.error.unwrap().contains(expected_msg));
    assert!(vm_o0.error.unwrap().contains(expected_msg));
    assert!(vm_o2.error.unwrap().contains(expected_msg));

    let reg = run_register_vm(source);
    assert!(reg.error.is_some());
    assert!(reg.error.unwrap().contains("Properties are not supported yet in the register backend."));
}

#[test]
fn test_error_undefined_method() {
    let source = "
class Empty {}
var obj = Empty();
obj.metodo_inexistente();
";

    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);

    assert!(tw.error.is_some());
    assert!(vm_o0.error.is_some());
    assert!(vm_o2.error.is_some());

    let expected_msg = "[line 4] Undefined property 'metodo_inexistente'.";
    assert!(tw.error.unwrap().contains(expected_msg));
    assert!(vm_o0.error.unwrap().contains(expected_msg));
    assert!(vm_o2.error.unwrap().contains(expected_msg));
}

#[test]
fn test_error_this_outside_class() {
    let source = "
print this;
";

    let stmts = parse_and_check(source);
    assert!(stmts.is_err());
    let err = stmts.unwrap_err();
    assert!(err.contains("[line 2] Error at 'this': Can't use 'this' outside of a class."));

    assert!(run_treewalk(source).error.is_some());
    assert!(run_stack_vm(source, OptLevel::O0).error.is_some());
    assert!(run_stack_vm(source, OptLevel::O2).error.is_some());

    let reg = run_register_vm(source);
    assert!(reg.error.is_some());
}

#[test]
fn test_error_super_outside_class() {
    let source = "
super.metodo();
";

    let stmts = parse_and_check(source);
    assert!(stmts.is_err());
    let err = stmts.unwrap_err();
    assert!(err.contains("[line 2] Error at 'super': Can't use 'super' outside of a class."));

    assert!(run_treewalk(source).error.is_some());
    assert!(run_stack_vm(source, OptLevel::O0).error.is_some());
    assert!(run_stack_vm(source, OptLevel::O2).error.is_some());
}

#[test]
fn test_error_super_without_superclass() {
    let source = "
class Standalone {
  foo() {
    super.foo();
  }
}
";

    let stmts = parse_and_check(source);
    assert!(stmts.is_err());
    let err = stmts.unwrap_err();
    assert!(err.contains("[line 4] Error at 'super': Can't use 'super' in a class with no superclass."));

    assert!(run_treewalk(source).error.is_some());
    assert!(run_stack_vm(source, OptLevel::O0).error.is_some());
    assert!(run_stack_vm(source, OptLevel::O2).error.is_some());
}

#[test]
fn test_error_self_inheritance() {
    let source = "
class Loop < Loop {}
";

    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let parse_res = parser.parse();
    assert!(parse_res.is_err());
    let parse_err = parse_res.unwrap_err();
    assert!(parse_err.contains("[line 2] Error at 'Loop': A class can't inherit from itself."));

    assert!(run_treewalk(source).error.is_some());
    assert!(run_stack_vm(source, OptLevel::O0).error.is_some());
    assert!(run_stack_vm(source, OptLevel::O2).error.is_some());
}

#[test]
fn test_error_inherit_non_class() {
    let source = "
var NotAClass = \"no_es_clase\";
class Child < NotAClass {}
";

    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);

    assert!(tw.error.is_some());
    assert!(vm_o0.error.is_some());
    assert!(vm_o2.error.is_some());

    let expected_msg = "[line 3] Superclass must be a class.";
    assert!(tw.error.unwrap().contains(expected_msg));
    assert!(vm_o0.error.unwrap().contains(expected_msg));
    assert!(vm_o2.error.unwrap().contains(expected_msg));
}

#[test]
fn test_error_return_from_init() {
    let source = "
class Account {
  init() {
    return 123;
  }
}
";

    let stmts = parse_and_check(source);
    assert!(stmts.is_err());
    let err = stmts.unwrap_err();
    assert!(err.contains("[line 4] Error at 'return': Can't return a value from an initializer."));

    assert!(run_treewalk(source).error.is_some());
    assert!(run_stack_vm(source, OptLevel::O0).error.is_some());
    assert!(run_stack_vm(source, OptLevel::O2).error.is_some());
}

#[test]
fn test_opt_prepare_invoke_direct_method_call() {
    let source = "
    class Greeter {
        hello(name) {
            return \"Hola \" + name;
        }
    }
    var g = Greeter();
    print g.hello(\"Mundo\");
    ";

    let stmts = parse_and_check(source).unwrap();

    // En -O0: no debe emitir PrepareInvoke ni Invoke, usa GetProperty + Call
    let proto_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0)).unwrap();
    assert!(!find_opcode(&proto_o0, &|op| matches!(op, OpCode::PrepareInvoke(_))));
    assert!(!find_opcode(&proto_o0, &|op| matches!(op, OpCode::Invoke(_))));
    assert!(find_opcode(&proto_o0, &|op| matches!(op, OpCode::GetProperty(_))));

    // En -O2: debe emitir PrepareInvoke e Invoke
    let proto_o2 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O2)).unwrap();
    assert!(find_opcode(&proto_o2, &|op| matches!(op, OpCode::PrepareInvoke(_))));
    assert!(find_opcode(&proto_o2, &|op| matches!(op, OpCode::Invoke(_))));

    // Salida identica en ambos niveles
    let res_o0 = run_stack_vm(source, OptLevel::O0);
    let res_o2 = run_stack_vm(source, OptLevel::O2);
    assert_eq!(res_o0.stdout, "Hola Mundo\n");
    assert_eq!(res_o2.stdout, "Hola Mundo\n");
}

#[test]
fn test_opt_prepare_super_invoke() {
    let source = "
    class Base {
        calc(x) { return x * 2; }
    }
    class Derived < Base {
        calc(x) { return super.calc(x) + 10; }
    }
    print Derived().calc(5);
    ";

    let stmts = parse_and_check(source).unwrap();

    // En -O0: no debe emitir PrepareSuperInvoke, usa GetSuper + Call
    let proto_o0 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O0)).unwrap();
    assert!(!find_opcode(&proto_o0, &|op| matches!(op, OpCode::PrepareSuperInvoke(_))));
    assert!(find_opcode(&proto_o0, &|op| matches!(op, OpCode::GetSuper(_))));

    // En -O2: debe emitir PrepareSuperInvoke e Invoke
    let proto_o2 = compiler::compile_with_opt(&stmts, OptConfig::from_level(OptLevel::O2)).unwrap();
    assert!(find_opcode(&proto_o2, &|op| matches!(op, OpCode::PrepareSuperInvoke(_))));

    // Salida identica en ambos niveles
    let res_o0 = run_stack_vm(source, OptLevel::O0);
    let res_o2 = run_stack_vm(source, OptLevel::O2);
    assert_eq!(res_o0.stdout, "20\n");
    assert_eq!(res_o2.stdout, "20\n");
}

#[test]
fn test_opt_field_shadowing_invokes_field_function() {
    let source = "
    class Greeter {
        greet() { return \"metodo original\"; }
    }
    var g = Greeter();
    fun custom() { return \"funcion en campo\"; }
    g.greet = custom;
    print g.greet();
    ";

    let tw = run_treewalk(source);
    let res_o0 = run_stack_vm(source, OptLevel::O0);
    let res_o2 = run_stack_vm(source, OptLevel::O2);

    assert_eq!(tw.stdout, "funcion en campo\n");
    assert_eq!(res_o0.stdout, "funcion en campo\n");
    assert_eq!(res_o2.stdout, "funcion en campo\n");
}

#[test]
fn test_opt_argument_evaluation_order_matches() {
    let source = "
    class OrderTester {
        method(a, b, c) {
            print a;
            print b;
            print c;
        }
    }
    fun f(x) {
        print \"eval: \" + x;
        return x;
    }
    var t = OrderTester();
    t.method(f(\"1\"), f(\"2\"), f(\"3\"));
    ";

    let tw = run_treewalk(source);
    let res_o0 = run_stack_vm(source, OptLevel::O0);
    let res_o2 = run_stack_vm(source, OptLevel::O2);

    let expected = "\
eval: 1
eval: 2
eval: 3
1
2
3
";
    assert_eq!(tw.stdout, expected);
    assert_eq!(res_o0.stdout, expected);
    assert_eq!(res_o2.stdout, expected);
}

#[test]
fn test_opt_closure_capturing_this() {
    let source = "
    class Box {
        init(val) {
            this.val = val;
        }
        getter() {
            fun get() {
                return this.val;
            }
            return get;
        }
    }
    var b = Box(42);
    var fn_get = b.getter();
    print fn_get();
    ";

    let tw = run_treewalk(source);
    let res_o0 = run_stack_vm(source, OptLevel::O0);
    let res_o2 = run_stack_vm(source, OptLevel::O2);

    assert_eq!(tw.stdout, "42\n");
    assert_eq!(res_o0.stdout, "42\n");
    assert_eq!(res_o2.stdout, "42\n");
}
