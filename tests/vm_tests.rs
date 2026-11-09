use rlox::checker;
use rlox::compiler;
use rlox::parser::Parser;
use rlox::scanner::Scanner;
use rlox::vm::Vm;

fn run_vm_output(source: &str) -> Result<String, String> {
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens()?;
    let mut parser = Parser::new(tokens);
    let statements = parser.parse()?;

    let errors = checker::check(&statements);
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    let proto = compiler::compile(&statements)?;
    let mut output = Vec::new();
    {
        let mut vm = Vm::with_output(&mut output);
        vm.interpret(proto)?;
    }
    String::from_utf8(output)
        .map(|s| s.replace("\r\n", "\n"))
        .map_err(|e| e.to_string())
}

fn run_vm(source: &str) -> Result<(), String> {
    run_vm_output(source).map(|_| ())
}

// ---------------------------------------------------------------------------
// Aritmética, precedencia y operadores
// ---------------------------------------------------------------------------

#[test]
fn test_vm_arithmetic_precedence_and_grouping() {
    let source = "
    print 10 + 5 * 2;
    print (10 + 5) * 2;
    print 20 - 4 / 2;
    print 17 % 5;
    print -(-7);
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "20\n30\n18\n2\n7\n");
}

#[test]
fn test_vm_comparisons_and_equality() {
    let source = "
    print 5 > 3;
    print 5 < 3;
    print 5 >= 5;
    print 5 <= 4;
    print 5 == 5;
    print 5 != 5;
    print \"abc\" == \"abc\";
    print \"abc\" != \"xyz\";
    print nil == nil;
    print nil == false;
    print !true;
    print !false;
    print !nil;
    print !\"texto\";
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(
        output,
        "true\nfalse\ntrue\nfalse\ntrue\nfalse\ntrue\ntrue\ntrue\nfalse\nfalse\ntrue\ntrue\nfalse\n"
    );
}

#[test]
fn test_vm_string_concatenation() {
    let source = "
    var greeting = \"Hello, \" + \"world!\";
    print greeting;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "Hello, world!\n");
}

// ---------------------------------------------------------------------------
// Variables, scopes y shadowing
// ---------------------------------------------------------------------------

#[test]
fn test_vm_globals_define_and_assign() {
    let source = "
    var a = \"inicial\";
    print a;
    a = \"reasignado\";
    print a;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "inicial\nreasignado\n");
}

#[test]
fn test_vm_locals_block_scoping_and_shadowing() {
    let source = "
    var a = \"global\";
    {
        var a = \"local\";
        print a;
        {
            var a = \"anidado\";
            print a;
        }
        print a;
    }
    print a;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "local\nanidado\nlocal\nglobal\n");
}

#[test]
fn test_vm_multiple_blocks_reuse_slots() {
    let source = "
    {
        var x = 10;
        var y = 20;
        print x + y;
    }
    {
        var z = 99;
        print z;
    }
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "30\n99\n");
}

// ---------------------------------------------------------------------------
// Control de flujo
// ---------------------------------------------------------------------------

#[test]
fn test_vm_if_else() {
    let source = "
    var x = 10;
    if (x > 5) {
        print \"mayor\";
    } else {
        print \"menor\";
    }

    if (x < 5) {
        print \"menor\";
    } else {
        print \"no menor\";
    }
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "mayor\nno menor\n");
}

#[test]
fn test_vm_logical_operators_short_circuit() {
    let source = "
    print false and \"no evaluado\";
    print true or \"no evaluado\";
    print nil or \"por defecto\";
    print \"izq\" and \"der\";
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "false\ntrue\npor defecto\nder\n");
}

#[test]
fn test_vm_while_loop() {
    let source = "
    var sum = 0;
    var i = 1;
    while (i <= 5) {
        sum = sum + i;
        i = i + 1;
    }
    print sum;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "15\n");
}

#[test]
fn test_vm_for_loop() {
    let source = "
    var total = 0;
    for (var i = 0; i < 4; i = i + 1) {
        total = total + i;
    }
    print total;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "6\n");
}

// ---------------------------------------------------------------------------
// Funciones, frames y llamadas
// ---------------------------------------------------------------------------

#[test]
fn test_vm_function_call_and_return() {
    let source = "
    fun add(a, b) {
        return a + b;
    }
    print add(3, 4);
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "7\n");
}

#[test]
fn test_vm_function_implicit_nil_return() {
    let source = "
    fun noReturn() {
        var x = 123;
    }
    print noReturn();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "nil\n");
}

#[test]
fn test_vm_function_early_return() {
    let source = "
    fun check(val) {
        if (val) return \"early\";
        return \"late\";
    }
    print check(true);
    print check(false);
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "early\nlate\n");
}

#[test]
fn test_vm_nested_calls_frame_restoration() {
    let source = "
    fun f1(x) { return x + 1; }
    fun f2(x) { return f1(x) * 2; }
    fun f3(x) { return f2(x) + 3; }
    print f3(5);
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "15\n");
}

#[test]
fn test_vm_recursion_factorial() {
    let source = "
    fun factorial(n) {
        if (n <= 1) return 1;
        return n * factorial(n - 1);
    }
    print factorial(5);
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "120\n");
}

#[test]
fn test_vm_recursion_fibonacci() {
    let source = "
    fun fib(n) {
        if (n <= 1) return n;
        return fib(n - 1) + fib(n - 2);
    }
    print fib(10);
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "55\n");
}

// ---------------------------------------------------------------------------
// Closures y captura de upvalues
// ---------------------------------------------------------------------------

#[test]
fn test_vm_counter_closure() {
    let source = "
    fun makeCounter() {
        var count = 0;
        fun counter() {
            count = count + 1;
            return count;
        }
        return counter;
    }
    var c = makeCounter();
    print c();
    print c();
    print c();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "1\n2\n3\n");
}

#[test]
fn test_vm_closure_closed_upvalues() {
    let source = "
    fun outer() {
        var x = \"closed value\";
        fun inner() {
            return x;
        }
        return inner;
    }
    var get = outer();
    print get();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "closed value\n");
}

#[test]
fn test_vm_closures_share_upvalue() {
    let source = "
    var get;
    var set;
    {
        var val = 10;
        fun readVal() { return val; }
        fun writeVal(v) { val = v; }
        get = readVal;
        set = writeVal;
    }
    print get();
    set(42);
    print get();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "10\n42\n");
}

#[test]
fn test_vm_multi_level_upvalues() {
    let source = "
    fun f1() {
        var a = 1;
        fun f2() {
            var b = 2;
            fun f3() {
                return a + b;
            }
            return f3;
        }
        return f2();
    }
    var fn3 = f1();
    print fn3();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "3\n");
}

// ---------------------------------------------------------------------------
// Errores de ejecución (Runtime Errors)
// ---------------------------------------------------------------------------

#[test]
fn test_vm_runtime_error_type_mismatch_subtract() {
    let source = "var res = \"texto\" - 5;";
    let result = run_vm(source);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("Operands must be numbers."),
        "Error inesperado: {err}"
    );
}

#[test]
fn test_vm_runtime_error_type_mismatch_negate() {
    let source = "var res = -\"texto\";";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Operand must be a number."));
}

#[test]
fn test_vm_runtime_error_undefined_variable_read() {
    let source = "print variableInexistente;";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Undefined variable 'variableInexistente'.")
    );
}

#[test]
fn test_vm_runtime_error_undefined_variable_assign() {
    let source = "variableInexistente = 123;";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Undefined variable 'variableInexistente'.")
    );
}

#[test]
fn test_vm_runtime_error_call_non_callable() {
    let source = "
    var notAFunction = 123;
    notAFunction();
    ";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Can only call functions and classes.")
    );
}

#[test]
fn test_vm_runtime_error_call_wrong_arity() {
    let source = "
    fun add(a, b) {
        return a + b;
    }
    add(1);
    ";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Expected 2 arguments but got 1.")
    );
}

// ---------------------------------------------------------------------------
// Tipos de runtime de la VM (Etapa 2)
// ---------------------------------------------------------------------------

#[test]
fn test_vm_runtime_types_display_and_equality() {
    use rlox::closure::{Closure, FnProto};
    use rlox::value::Value;
    use rlox::vm_class::{BoundMethod, VmClass, VmInstance};
    use std::cell::RefCell;
    use std::rc::Rc;

    let class_rc1 = Rc::new(RefCell::new(VmClass::new("Bagel".to_string())));
    let class_rc2 = Rc::new(RefCell::new(VmClass::new("Bagel".to_string())));

    let val_class1 = Value::VmClass(Rc::clone(&class_rc1));
    let val_class1_copy = Value::VmClass(Rc::clone(&class_rc1));
    let val_class2 = Value::VmClass(Rc::clone(&class_rc2));

    assert_eq!(format!("{}", val_class1), "<class Bagel>");
    assert_eq!(val_class1, val_class1_copy);
    assert_ne!(val_class1, val_class2);
    assert!(val_class1.is_truthy());

    let inst_rc1 = Rc::new(RefCell::new(VmInstance::new(Rc::clone(&class_rc1))));
    let inst_rc2 = Rc::new(RefCell::new(VmInstance::new(Rc::clone(&class_rc1))));

    let val_inst1 = Value::VmInstance(Rc::clone(&inst_rc1));
    let val_inst1_copy = Value::VmInstance(Rc::clone(&inst_rc1));
    let val_inst2 = Value::VmInstance(Rc::clone(&inst_rc2));

    assert_eq!(format!("{}", val_inst1), "<instance of Bagel>");
    assert_eq!(val_inst1, val_inst1_copy);
    assert_ne!(val_inst1, val_inst2);
    assert!(val_inst1.is_truthy());

    let proto = Rc::new(FnProto::new(Some("eat".to_string())));
    let closure = Rc::new(Closure::new(proto));
    let bound1 = Rc::new(BoundMethod::new(val_inst1.clone(), Rc::clone(&closure)));
    let bound2 = Rc::new(BoundMethod::new(val_inst1.clone(), Rc::clone(&closure)));

    let val_bound1 = Value::BoundMethod(Rc::clone(&bound1));
    let val_bound1_copy = Value::BoundMethod(Rc::clone(&bound1));
    let val_bound2 = Value::BoundMethod(Rc::clone(&bound2));

    assert_eq!(format!("{}", val_bound1), "<fn eat>");
    assert_eq!(val_bound1, val_bound1_copy);
    assert_ne!(val_bound1, val_bound2);
    assert!(val_bound1.is_truthy());
}
