//! Tests exhaustivos para operadores de asignación compuesta (+=, -=, *=, /=, %=) en rlox.
//!
//! Cubre:
//! - Los 5 operadores (+-, -=, *=, /=, %=) sobre números y cadenas (+).
//! - Variables locales, globales y variables capturadas por clausuras (upvalues).
//! - Propiedades de instancias y métodos que mutan vía 'this'.
//! - Evaluación única del receptor (obtenerObjeto().contador += inc()).
//! - Orden de evaluación estricto de subexpresiones de izquierda a derecha.
//! - Valor de retorno de la expresión de asignación.
//! - Asignaciones encadenadas y asociatividad.
//! - Precedencia con respecto a otros operadores aritméticos.
//! - Operandos inválidos y errores de tiempo de ejecución.
//! - Destinos de asignación inválidos en tiempo de compilación/parseo.
//! - Comportamiento de %= con números negativos y cero.
//! - Paridad entre Tree-Walk, Stack VM (-O0, -O1, -O2) y Register VM (variables).
//! - Rechazo explícito de propiedades en Register VM.

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

/// Helper para verificar paridad entre los 4 backends (TreeWalk, Stack VM -O0, Stack VM -O2 y Register VM)
/// para operaciones sobre variables.
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

/// Helper para verificar paridad entre TreeWalk, Stack VM -O0 y Stack VM -O2 para propiedades de clases,
/// verificando ademas que Register VM rechaza propiedades de manera explicita.
fn assert_class_backends_ok(source: &str, expected_stdout: &str) {
    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);

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

    assert_eq!(tw.stdout, expected_stdout, "TreeWalk stdout no coincide");
    assert_eq!(vm_o0.stdout, expected_stdout, "VM -O0 stdout no coincide");
    assert_eq!(vm_o2.stdout, expected_stdout, "VM -O2 stdout no coincide");

    // Register VM debe rechazar propiedades o clases explicitamente
    let reg = run_register_vm(source);
    assert!(
        reg.error.is_some(),
        "Register VM debio fallar ante clases/propiedades"
    );
    let reg_err = reg.error.unwrap();
    assert!(
        reg_err.contains("Classes are not supported")
            || reg_err.contains("Properties are not supported"),
        "Register VM emitio un error no esperado: {}",
        reg_err
    );
}

/// Helper para verificar errores de tiempo de ejecucion en todos los backends que soportan variables.
fn assert_all_backends_runtime_error(source: &str, expected_line: usize, expected_msg: &str) {
    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);
    let reg = run_register_vm(source);

    let expected_pattern = format!("[line {}] {}", expected_line, expected_msg);

    assert!(tw.error.is_some(), "TreeWalk debio fallar");
    assert!(
        tw.error.as_ref().unwrap().contains(&expected_pattern),
        "TreeWalk error no esperado: {:?}",
        tw.error
    );

    assert!(vm_o0.error.is_some(), "VM -O0 debio fallar");
    assert!(
        vm_o0.error.as_ref().unwrap().contains(&expected_pattern),
        "VM -O0 error no esperado: {:?}",
        vm_o0.error
    );

    assert!(vm_o2.error.is_some(), "VM -O2 debio fallar");
    assert!(
        vm_o2.error.as_ref().unwrap().contains(&expected_pattern),
        "VM -O2 error no esperado: {:?}",
        vm_o2.error
    );

    assert!(reg.error.is_some(), "Register VM debio fallar");
    assert!(
        reg.error.as_ref().unwrap().contains(&expected_pattern),
        "Register VM error no esperado: {:?}",
        reg.error
    );
}

/// Helper para verificar errores de tiempo de ejecucion en backends que soportan clases.
fn assert_class_backends_runtime_error(source: &str, expected_line: usize, expected_msg: &str) {
    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);

    let expected_pattern = format!("[line {}] {}", expected_line, expected_msg);

    assert!(tw.error.is_some(), "TreeWalk debio fallar");
    assert!(
        tw.error.as_ref().unwrap().contains(&expected_pattern),
        "TreeWalk error no esperado: {:?}",
        tw.error
    );

    assert!(vm_o0.error.is_some(), "VM -O0 debio fallar");
    assert!(
        vm_o0.error.as_ref().unwrap().contains(&expected_pattern),
        "VM -O0 error no esperado: {:?}",
        vm_o0.error
    );

    assert!(vm_o2.error.is_some(), "VM -O2 debio fallar");
    assert!(
        vm_o2.error.as_ref().unwrap().contains(&expected_pattern),
        "VM -O2 error no esperado: {:?}",
        vm_o2.error
    );
}

#[test]
fn test_compound_assign_all_operators_global() {
    let source = r#"
        var a = 10;
        a += 5;
        print a;
        a -= 3;
        print a;
        a *= 4;
        print a;
        a /= 6;
        print a;
        a %= 5;
        print a;
    "#;
    assert_all_backends_ok(source, "15\n12\n48\n8\n3\n");
}

#[test]
fn test_compound_assign_all_operators_local() {
    let source = r#"
        {
            var x = 100;
            x += 50;
            print x;
            x -= 30;
            print x;
            x *= 2;
            print x;
            x /= 4;
            print x;
            x %= 17;
            print x;
        }
    "#;
    assert_all_backends_ok(source, "150\n120\n240\n60\n9\n");
}

#[test]
fn test_compound_assign_string_concatenation() {
    let source = r#"
        var s = "hola";
        s += " mundo";
        print s;
        {
            var t = "rlox";
            t += "!";
            print t;
        }
    "#;
    assert_all_backends_ok(source, "hola mundo\nrlox!\n");
}

#[test]
fn test_compound_assign_modulo_negative_and_floats() {
    let source = r#"
        var a = -10;
        a %= 3;
        print a;

        var b = 10;
        b %= -3;
        print b;

        var c = 7.5;
        c %= 2.5;
        print c;

        var d = 10;
        d %= 0;
        print d;
    "#;
    assert_all_backends_ok(source, "-1\n1\n0\nNaN\n");
}

#[test]
fn test_compound_assign_closure_upvalue() {
    let source = r#"
        fun makeCounter() {
            var count = 0;
            fun inc(step) {
                count += step;
                return count;
            }
            return inc;
        }

        var counter = makeCounter();
        print counter(5);
        print counter(10);
        print counter(-3);
    "#;
    assert_all_backends_ok(source, "5\n15\n12\n");
}

#[test]
fn test_compound_assign_multiple_closures_share_upvalue() {
    let source = r#"
        fun makeBank(initial) {
            var balance = initial;
            fun deposit(amount) {
                balance += amount;
                return balance;
            }
            fun withdraw(amount) {
                balance -= amount;
                return balance;
            }
            fun getBalance() {
                return balance;
            }

            deposit(100);
            withdraw(20);
            print getBalance();
        }

        makeBank(50);
    "#;
    assert_all_backends_ok(source, "130\n");
}

#[test]
fn test_compound_assign_return_value() {
    let source = r#"
        var x = 10;
        print (x += 5);
        print x;

        var y = 20;
        var z = (y *= 2);
        print y;
        print z;
    "#;
    assert_all_backends_ok(source, "15\n15\n40\n40\n");
}

#[test]
fn test_compound_assign_chained() {
    let source = r#"
        var a = 10;
        var b = 5;
        a += b += 3;
        print a;
        print b;
    "#;
    assert_all_backends_ok(source, "18\n8\n");
}

#[test]
fn test_compound_assign_precedence_and_grouping() {
    let source = r#"
        var x = 10;
        x += 2 * 3;
        print x;

        var y = 20;
        y -= 4 / 2;
        print y;
    "#;
    assert_all_backends_ok(source, "16\n18\n");
}

#[test]
fn test_compound_assign_in_loops() {
    let source = r#"
        var sum = 0;
        for (var i = 1; i <= 5; i += 1) {
            sum += i;
        }
        print sum;

        var prod = 1;
        var j = 1;
        while (j <= 4) {
            prod *= j;
            j += 1;
        }
        print prod;
    "#;
    assert_all_backends_ok(source, "15\n24\n");
}

#[test]
fn test_compound_assign_property_basic() {
    let source = r#"
        class Counter {
            init() {
                this.value = 0;
            }
            add(n) {
                this.value += n;
            }
        }

        var c = Counter();
        c.value += 10;
        print c.value;
        c.add(5);
        print c.value;
        c.value *= 2;
        print c.value;
        c.value %= 7;
        print c.value;
    "#;
    assert_class_backends_ok(source, "10\n15\n30\n2\n");
}

#[test]
fn test_compound_assign_property_string_concatenation() {
    let source = r#"
        class Message {
            init(head) {
                this.text = head;
            }
        }

        var m = Message("Hola");
        m.text += " mundo";
        m.text += "!";
        print m.text;
    "#;
    assert_class_backends_ok(source, "Hola mundo!\n");
}

#[test]
fn test_compound_assign_property_return_value() {
    let source = r#"
        class Item {
            init() {
                this.price = 50;
            }
        }

        var item = Item();
        print (item.price += 25);
        print item.price;
    "#;
    assert_class_backends_ok(source, "75\n75\n");
}

#[test]
fn test_compound_assign_single_receiver_evaluation() {
    let source = r#"
        var receiverCalls = 0;

        class Box {
            init() {
                this.item = 10;
            }
        }

        var theBox = Box();

        fun getBox() {
            receiverCalls += 1;
            return theBox;
        }

        getBox().item += 5;

        print receiverCalls;
        print theBox.item;
    "#;
    // getBox() debe ejecutarse EXACTAMENTE una vez.
    assert_class_backends_ok(source, "1\n15\n");
}

#[test]
fn test_compound_assign_receiver_and_value_evaluation_order() {
    let source = r#"
        class State {
            init() {
                this.val = 100;
            }
        }

        var s = State();

        fun getReceiver() {
            print "receiver";
            return s;
        }

        fun getValue() {
            print "value";
            return 25;
        }

        getReceiver().val += getValue();
        print s.val;
    "#;
    // El orden debe ser estrictamente:
    // 1. receiver
    // 2. value
    // Resultado: 125
    assert_class_backends_ok(source, "receiver\nvalue\n125\n");
}

#[test]
fn test_compound_assign_error_on_non_instance_receiver() {
    let source = r#"
        fun sideEffect() {
            print "no deberia ejecutarse";
            return 1;
        }

        var num = 123;
        num.field += sideEffect();
    "#;
    // num no es instancia: debe fallar antes de evaluar sideEffect()
    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);

    assert!(
        tw.stdout.is_empty(),
        "TreeWalk ejecuto efectos secundarios indebidos"
    );
    assert!(
        vm_o0.stdout.is_empty(),
        "VM -O0 ejecuto efectos secundarios indebidos"
    );
    assert!(
        vm_o2.stdout.is_empty(),
        "VM -O2 ejecuto efectos secundarios indebidos"
    );

    assert_class_backends_runtime_error(source, 8, "Only instances have fields.");
}

#[test]
fn test_compound_assign_error_undefined_property_before_value() {
    let source = r#"
        class Dummy {}
        var d = Dummy();

        fun sideEffect() {
            print "no deberia ejecutarse";
            return 5;
        }

        d.nonExistent += sideEffect();
    "#;
    // d no tiene la propiedad 'nonExistent': debe fallar antes de evaluar sideEffect()
    let tw = run_treewalk(source);
    let vm_o0 = run_stack_vm(source, OptLevel::O0);
    let vm_o2 = run_stack_vm(source, OptLevel::O2);

    assert!(
        tw.stdout.is_empty(),
        "TreeWalk ejecuto efectos secundarios indebidos"
    );
    assert!(
        vm_o0.stdout.is_empty(),
        "VM -O0 ejecuto efectos secundarios indebidos"
    );
    assert!(
        vm_o2.stdout.is_empty(),
        "VM -O2 ejecuto efectos secundarios indebidos"
    );

    assert_class_backends_runtime_error(source, 10, "Undefined property 'nonExistent'.");
}

#[test]
fn test_compound_assign_invalid_operand_types() {
    let source = r#"
        var a = "hola";
        a -= 5;
    "#;
    assert_all_backends_runtime_error(source, 3, "Operands must be numbers.");

    let source2 = r#"
        var b = true;
        b += 10;
    "#;
    assert_all_backends_runtime_error(source2, 3, "Operands must be two numbers or two strings.");

    let source3 = r#"
        var c = 10;
        c += "chau";
    "#;
    assert_all_backends_runtime_error(source3, 3, "Operands must be two numbers or two strings.");
}

#[test]
fn test_compound_assign_invalid_assignment_targets() {
    let source1 = "1 += 2;";
    let res1 = parse_and_check(source1);
    assert!(res1.is_err());
    assert!(res1.unwrap_err().contains("Invalid assignment target."));

    let source2 = "(a + b) += 1;";
    let res2 = parse_and_check(source2);
    assert!(res2.is_err());
    assert!(res2.unwrap_err().contains("Invalid assignment target."));
}

#[test]
fn test_compound_assign_read_in_own_initializer() {
    let source = r#"
        {
            var a = (a += 1);
        }
    "#;
    let res = parse_and_check(source);
    assert!(res.is_err());
    assert!(
        res.unwrap_err()
            .contains("Can't read local variable in its own initializer.")
    );
}
