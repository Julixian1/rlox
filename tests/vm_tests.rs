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

// ---------------------------------------------------------------------------
// Clases, instancias, campos y orden de evaluación en Set (Etapa 3)
// ---------------------------------------------------------------------------

#[test]
fn test_vm_class_declaration_and_instantiation() {
    let source = "
    class Bagel {}
    print Bagel;
    var bagel = Bagel();
    print bagel;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "<class Bagel>\n<instance of Bagel>\n");
}

#[test]
fn test_vm_class_local_and_nested_in_function() {
    let source = "
    {
        class LocalClass {}
        var l = LocalClass();
        print l;
    }
    fun makeInstance() {
        class FunctionClass {}
        return FunctionClass();
    }
    var f = makeInstance();
    print f;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(
        output,
        "<instance of LocalClass>\n<instance of FunctionClass>\n"
    );
}

#[test]
fn test_vm_field_get_and_set() {
    let source = "
    class Box {}
    var box = Box();
    box.width = 10;
    box.height = 20;
    print box.width;
    print box.height;
    print box.width + box.height;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "10\n20\n30\n");
}

#[test]
fn test_vm_field_assignment_expression_value() {
    let source = "
    class Box {}
    var box = Box();
    print box.value = 42;
    print box.value;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "42\n42\n");
}

#[test]
fn test_vm_fields_are_independent_across_instances() {
    let source = "
    class Point {}
    var p1 = Point();
    var p2 = Point();
    p1.x = 100;
    p2.x = 200;
    print p1.x;
    print p2.x;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "100\n200\n");
}

#[test]
fn test_vm_instances_compared_by_identity() {
    let source = "
    class Foo {}
    var f1 = Foo();
    var f2 = Foo();
    print f1 == f1;
    print f1 == f2;
    print f1 != f2;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "true\nfalse\ntrue\n");
}

#[test]
fn test_vm_error_get_property_on_non_instance() {
    let source = "\"no es instancia\".propiedad;";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Only instances have properties.")
    );
}

#[test]
fn test_vm_error_undefined_property() {
    let source = "
    class Foo {}
    var foo = Foo();
    print foo.inexistente;
    ";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Undefined property 'inexistente'.")
    );
}

#[test]
fn test_vm_error_class_instantiation_wrong_arity() {
    let source = "
    class Foo {}
    Foo(1, 2);
    ";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Expected 0 arguments but got 2.")
    );
}

#[test]
fn test_vm_set_evaluation_order_aborts_before_value() {
    let source = "
    fun sideEffect() {
        print \"EFECTO SECUNDARIO EJECUTADO\";
        return 123;
    }
    var x = \"no instancia\";
    x.campo = sideEffect();
    ";
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().unwrap();
    assert!(checker::check(&statements).is_empty());
    let proto = compiler::compile(&statements).unwrap();

    let mut output = Vec::new();
    let result = {
        let mut vm = Vm::with_output(&mut output);
        vm.interpret(proto)
    };
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Only instances have fields."));
    let stdout = String::from_utf8(output).unwrap();
    assert!(
        !stdout.contains("EFECTO SECUNDARIO EJECUTADO"),
        "El efecto secundario se ejecutó indebidamente"
    );
}

// ---------------------------------------------------------------------------
// Métodos, this e inicializadores (Etapa 4)
// ---------------------------------------------------------------------------

#[test]
fn test_vm_method_declaration_and_invocation() {
    let source = "
    class Greeter {
        greet() {
            print \"hola mundo\";
        }
    }
    var g = Greeter();
    g.greet();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "hola mundo\n");
}

#[test]
fn test_vm_method_access_and_modify_fields_via_this() {
    let source = "
    class Cake {
        setFlavor(flavor) {
            this.flavor = flavor;
        }
        describe() {
            print \"Pastel de \" + this.flavor;
        }
    }
    var cake = Cake();
    cake.setFlavor(\"chocolate\");
    cake.describe();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "Pastel de chocolate\n");
}

#[test]
fn test_vm_extracted_method_retains_receiver() {
    let source = "
    class Person {
        sayName() {
            print this.name;
        }
    }
    var p = Person();
    p.name = \"Alice\";
    var fn_ptr = p.sayName;
    fn_ptr();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "Alice\n");
}

#[test]
fn test_vm_field_shadows_method() {
    let source = "
    class Foo {
        bar() {
            print \"metodo\";
        }
    }
    var foo = Foo();
    foo.bar = \"campo\";
    print foo.bar;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "campo\n");
}

#[test]
fn test_vm_initializer_with_arguments_and_implicit_return() {
    let source = "
    class Point {
        init(x, y) {
            this.x = x;
            this.y = y;
        }
    }
    var p = Point(3, 4);
    print p.x;
    print p.y;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "3\n4\n");
}

#[test]
fn test_vm_initializer_explicit_empty_return() {
    let source = "
    class Foo {
        init(early) {
            this.val = \"normal\";
            if (early) return;
            this.val = \"modificado\";
        }
    }
    var f1 = Foo(true);
    var f2 = Foo(false);
    print f1.val;
    print f2.val;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "normal\nmodificado\n");
}

#[test]
fn test_vm_initializer_direct_invocation_returns_instance() {
    let source = "
    class Foo {
        init(val) {
            this.val = val;
        }
    }
    var foo = Foo(1);
    print foo.val;
    var ret = foo.init(2);
    print foo.val;
    print ret.val;
    print foo == ret;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "1\n2\n2\ntrue\n");
}

#[test]
fn test_vm_initializer_wrong_arity() {
    let source = "
    class Foo {
        init(a, b) {}
    }
    Foo(1);
    ";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Expected 2 arguments but got 1.")
    );
}

#[test]
fn test_vm_this_captured_by_nested_function() {
    let source = "
    class Counter {
        init() {
            this.count = 0;
        }
        getIncrementer() {
            fun inc() {
                this.count = this.count + 1;
                return this.count;
            }
            return inc;
        }
    }
    var c = Counter();
    var inc = c.getIncrementer();
    print inc();
    print inc();
    print c.count;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "1\n2\n2\n");
}

#[test]
fn test_vm_normal_function_inside_initializer_can_return_value() {
    let source = "
    class Foo {
        init() {
            fun helper() {
                return 42;
            }
            this.val = helper();
        }
    }
    var f = Foo();
    print f.val;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "42\n");
}

#[test]
fn test_vm_initializer_propagates_runtime_error() {
    let source = "
    class Foo {
        init() {
            var x = \"bad\" - 5;
        }
    }
    Foo();
    ";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Operands must be numbers."));
}

#[test]
fn test_vm_bound_methods_compared_by_identity() {
    let source = "
    class Foo {
        method() {}
    }
    var foo = Foo();
    var m1 = foo.method;
    var m2 = foo.method;
    print m1 == m1;
    print m1 == m2;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "true\nfalse\n");
}

// ---------------------------------------------------------------------------
// Herencia y super (Etapa 5)
// ---------------------------------------------------------------------------

#[test]
fn test_vm_simple_inheritance() {
    let source = "
    class Doughnut {
        cook() {
            print \"Fry until golden brown.\";
        }
    }
    class BostonCream < Doughnut {}
    BostonCream().cook();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "Fry until golden brown.\n");
}

#[test]
fn test_vm_method_override() {
    let source = "
    class Doughnut {
        cook() {
            print \"Fry until golden brown.\";
        }
    }
    class BostonCream < Doughnut {
        cook() {
            print \"Pipe full of custard and coat with chocolate.\";
        }
    }
    BostonCream().cook();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "Pipe full of custard and coat with chocolate.\n");
}

#[test]
fn test_vm_super_method_call() {
    let source = "
    class Doughnut {
        cook() {
            print \"Doughnut cook\";
        }
    }
    class BostonCream < Doughnut {
        cook() {
            super.cook();
            print \"BostonCream cook\";
        }
    }
    BostonCream().cook();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "Doughnut cook\nBostonCream cook\n");
}

#[test]
fn test_vm_inherited_initializer() {
    let source = "
    class Parent {
        init(name) {
            this.name = name;
        }
    }
    class Child < Parent {}
    var c = Child(\"Alex\");
    print c.name;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "Alex\n");
}

#[test]
fn test_vm_inherited_initializer_wrong_arity() {
    let source = "
    class Parent {
        init(a, b) {
            this.a = a;
            this.b = b;
        }
    }
    class Child < Parent {}
    Child(1);
    ";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Expected 2 arguments but got 1.")
    );
}

#[test]
fn test_vm_three_level_super_maintains_original_receiver() {
    let source = "
    class A {
        method() {
            print this.name;
        }
    }
    class B < A {
        method() {
            super.method();
        }
    }
    class C < B {
        method() {
            super.method();
        }
    }
    var c = C();
    c.name = \"Instancia de C\";
    c.method();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "Instancia de C\n");
}

#[test]
fn test_vm_super_and_this_captured_in_closure() {
    let source = "
    class Base {
        greet() {
            return \"Base\";
        }
    }
    class Derived < Base {
        getGreeter() {
            fun greeter() {
                return super.greet() + \" for \" + this.name;
            }
            return greeter;
        }
    }
    var d = Derived();
    d.name = \"Alice\";
    var fn_greeter = d.getGreeter();
    print fn_greeter();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "Base for Alice\n");
}

#[test]
fn test_vm_local_class_inheritance() {
    let source = "
    var result = \"\";
    {
        class LocalBase {
            say() {
                return \"base\";
            }
        }
        class LocalSub < LocalBase {
            say() {
                return super.say() + \" sub\";
            }
        }
        var s = LocalSub();
        result = s.say();
    }
    print result;
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "base sub\n");
}

#[test]
fn test_vm_class_inheritance_in_function() {
    let source = "
    fun makeSub() {
        class Base {
            msg() {
                return \"base msg\";
            }
        }
        class Sub < Base {
            msg() {
                return super.msg() + \" extended\";
            }
        }
        return Sub();
    }
    var inst = makeSub();
    print inst.msg();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "base msg extended\n");
}

#[test]
fn test_vm_nested_classes_independent_contexts() {
    let source = "
    class Super {
        foo() {
            print \"Super foo\";
        }
    }
    class Outer < Super {
        getInner() {
            class Inner {
                bar() {
                    print \"Inner bar\";
                }
            }
            return Inner();
        }
        foo() {
            super.foo();
        }
    }
    var o = Outer();
    o.foo();
    var i = o.getInner();
    i.bar();
    ";
    let output = run_vm_output(source).expect("ejecución exitosa");
    assert_eq!(output, "Super foo\nInner bar\n");
}

#[test]
fn test_vm_runtime_error_superclass_not_a_class() {
    let source = "
    var NotClass = \"I am a string\";
    class Sub < NotClass {}
    ";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Superclass must be a class."));
}

#[test]
fn test_vm_runtime_error_undefined_super_property() {
    let source = "
    class Base {}
    class Sub < Base {
        foo() {
            super.nonExistent();
        }
    }
    Sub().foo();
    ";
    let result = run_vm(source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Undefined property 'nonExistent'.")
    );
}
