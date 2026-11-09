use rlox::checker;
use rlox::compiler;
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
pub struct ExecutionResult {
    pub stdout: String,
    pub error: Option<String>,
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

fn run_treewalk(source: &str) -> ExecutionResult {
    let statements = match parse_and_check(source) {
        Ok(s) => s,
        Err(e) => {
            return ExecutionResult {
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
        String::from_utf8(bytes).unwrap_or_default()
    };

    ExecutionResult { stdout, error: err }
}

fn run_vm(source: &str) -> ExecutionResult {
    let statements = match parse_and_check(source) {
        Ok(s) => s,
        Err(e) => {
            return ExecutionResult {
                stdout: String::new(),
                error: Some(e),
            };
        }
    };

    let proto = match compiler::compile(&statements) {
        Ok(p) => p,
        Err(e) => {
            return ExecutionResult {
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
    let stdout = String::from_utf8(output).unwrap_or_default();

    ExecutionResult { stdout, error: err }
}

fn assert_parity_ok(source: &str) {
    let tw = run_treewalk(source);
    let vm = run_vm(source);

    assert!(
        tw.error.is_none(),
        "TreeWalk falló inesperadamente: {:?}",
        tw.error
    );
    assert!(
        vm.error.is_none(),
        "VM falló inesperadamente: {:?}",
        vm.error
    );
    assert_eq!(
        tw.stdout, vm.stdout,
        "Diferencia en salida estándar entre TreeWalk y VM.\n--- TreeWalk ---\n{}\n--- VM ---\n{}",
        tw.stdout, vm.stdout
    );
}

fn assert_parity_err(source: &str, expected_substring: &str) {
    let tw = run_treewalk(source);
    let vm = run_vm(source);

    assert!(
        tw.error.is_some(),
        "TreeWalk debió fallar pero tuvo éxito. Salida:\n{}",
        tw.stdout
    );
    assert!(
        vm.error.is_some(),
        "VM debió fallar pero tuvo éxito. Salida:\n{}",
        vm.stdout
    );

    let tw_err = tw.error.unwrap();
    let vm_err = vm.error.unwrap();

    assert!(
        tw_err.contains(expected_substring),
        "Error de TreeWalk ('{}') no contiene '{}'",
        tw_err,
        expected_substring
    );
    assert!(
        vm_err.contains(expected_substring),
        "Error de VM ('{}') no contiene '{}'",
        vm_err,
        expected_substring
    );
}

// ===========================================================================
// Test del escritor inyectado
// ===========================================================================

#[test]
fn test_parity_injected_writer_captures_multiple_prints() {
    let source = "
    print \"linea 1\";
    print 2 + 3;
    print true;
    print nil;
    ";
    let tw = run_treewalk(source);
    let vm = run_vm(source);
    assert_eq!(tw.stdout, "linea 1\n5\ntrue\nnil\n");
    assert_eq!(vm.stdout, "linea 1\n5\ntrue\nnil\n");
}

// ===========================================================================
// A. Clases e instancias
// ===========================================================================

#[test]
fn test_parity_global_class_and_instance() {
    let source = "
    class Bagel {}
    print Bagel;
    var bagel = Bagel();
    print bagel;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_local_class_in_block() {
    let source = "
    {
        class LocalBox {}
        var b = LocalBox();
        print b;
    }
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_class_returned_from_function() {
    let source = "
    fun makeClass() {
        class Product {}
        return Product();
    }
    var p = makeClass();
    print p;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_independent_instances() {
    let source = "
    class Point {}
    var p1 = Point();
    var p2 = Point();
    p1.x = 10;
    p2.x = 20;
    print p1.x;
    print p2.x;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_dynamic_fields() {
    let source = "
    class Entity {}
    var e = Entity();
    e.name = \"heroe\";
    e.hp = 100;
    e.alive = true;
    print e.name;
    print e.hp;
    print e.alive;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_field_assignment_expression_value() {
    let source = "
    class Box {}
    var b = Box();
    print b.val = 42;
    print b.val;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_identity_equality() {
    let source = "
    class Foo {}
    var f1 = Foo();
    var f2 = Foo();
    var f3 = f1;
    print f1 == f1;
    print f1 == f3;
    print f1 == f2;
    print f1 != f2;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_runtime_error_undefined_property() {
    let source = "
    class Foo {}
    var f = Foo();
    print f.noExiste;
    ";
    assert_parity_err(source, "Undefined property 'noExiste'.");
}

// ===========================================================================
// B. Métodos y this
// ===========================================================================

#[test]
fn test_parity_method_declaration_and_invocation() {
    let source = "
    class Greeter {
        sayHello(name) {
            print \"Hola \" + name;
        }
    }
    var g = Greeter();
    g.sayHello(\"Lox\");
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_method_reads_and_writes_fields_via_this() {
    let source = "
    class Counter {
        set(n) {
            this.val = n;
        }
        get() {
            return this.val;
        }
    }
    var c = Counter();
    c.set(99);
    print c.get();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_extracted_method_retains_receiver() {
    let source = "
    class Person {
        speak() {
            print this.nombre;
        }
    }
    var p = Person();
    p.nombre = \"Carlos\";
    var fn_ptr = p.speak;
    fn_ptr();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_field_shadows_method() {
    let source = "
    class Car {
        drive() {
            return \"manejando\";
        }
    }
    var c = Car();
    print c.drive();
    c.drive = \"campo sombreado\";
    print c.drive;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_method_returns_closure() {
    let source = "
    class Multiplier {
        getMultiplier(factor) {
            fun mul(n) {
                return n * factor;
            }
            return mul;
        }
    }
    var m = Multiplier();
    var triple = m.getMultiplier(3);
    print triple(7);
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_closure_captures_this() {
    let source = "
    class Accumulator {
        init() {
            this.total = 0;
        }
        getAdder() {
            fun add(n) {
                this.total = this.total + n;
                return this.total;
            }
            return add;
        }
    }
    var a = Accumulator();
    var adder = a.getAdder();
    print adder(5);
    print adder(10);
    print a.total;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_local_class_with_methods() {
    let source = "
    {
        class Greeter {
            greet() {
                return \"saludo local\";
            }
        }
        var g = Greeter();
        print g.greet();
    }
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_method_invoking_another_method() {
    let source = "
    class Calculator {
        add(a, b) {
            return a + b;
        }
        addDouble(a, b) {
            return this.add(a, b) * 2;
        }
    }
    var calc = Calculator();
    print calc.addDouble(3, 4);
    ";
    assert_parity_ok(source);
}

// ===========================================================================
// C. Inicializadores
// ===========================================================================

#[test]
fn test_parity_initializer_without_parameters() {
    let source = "
    class Foo {
        init() {
            this.creado = true;
        }
    }
    var f = Foo();
    print f.creado;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_initializer_with_parameters() {
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
    assert_parity_ok(source);
}

#[test]
fn test_parity_initializer_explicit_return_empty() {
    let source = "
    class Guard {
        init(early) {
            this.val = \"normal\";
            if (early) return;
            this.val = \"completo\";
        }
    }
    var g1 = Guard(true);
    var g2 = Guard(false);
    print g1.val;
    print g2.val;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_initializer_direct_invocation() {
    let source = "
    class Item {
        init(name) {
            this.name = name;
        }
    }
    var it = Item(\"A\");
    print it.name;
    var ret = it.init(\"B\");
    print it.name;
    print ret.name;
    print it == ret;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_initializer_wrong_arity() {
    let source = "
    class Point {
        init(x, y) {
            this.x = x;
            this.y = y;
        }
    }
    Point(1);
    ";
    assert_parity_err(source, "Expected 2 arguments but got 1.");
}

#[test]
fn test_parity_initializer_executes_closure() {
    let source = "
    class Setup {
        init() {
            fun compute() {
                return 100 + 23;
            }
            this.code = compute();
        }
    }
    var s = Setup();
    print s.code;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_initializer_runtime_error_propagates() {
    let source = "
    class Broken {
        init() {
            var x = \"bad\" - 5;
        }
    }
    Broken();
    ";
    assert_parity_err(source, "Operands must be numbers.");
}

// ===========================================================================
// D. Herencia y super
// ===========================================================================

#[test]
fn test_parity_simple_inheritance() {
    let source = "
    class Super {
        hello() {
            return \"hello from super\";
        }
    }
    class Sub < Super {}
    var s = Sub();
    print s.hello();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_method_override() {
    let source = "
    class Super {
        tag() {
            return \"super\";
        }
    }
    class Sub < Super {
        tag() {
            return \"sub\";
        }
    }
    var s = Sub();
    print s.tag();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_super_method_call() {
    let source = "
    class A {
        m() {
            return \"A\";
        }
    }
    class B < A {
        m() {
            return \"B \" + super.m();
        }
    }
    print B().m();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_inherited_initializer() {
    let source = "
    class Base {
        init(id) {
            this.id = id;
        }
    }
    class Child < Base {}
    var c = Child(42);
    print c.id;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_override_init_with_super_init() {
    let source = "
    class Base {
        init(id) {
            this.id = id;
        }
    }
    class Child < Base {
        init(id, label) {
            super.init(id);
            this.label = label;
        }
    }
    var c = Child(10, \"item\");
    print c.id;
    print c.label;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_three_level_hierarchy() {
    let source = "
    class A {
        level() {
            return \"A\";
        }
    }
    class B < A {
        level() {
            return \"B \" + super.level();
        }
    }
    class C < B {
        level() {
            return \"C \" + super.level();
        }
    }
    print C().level();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_three_level_super_maintains_original_receiver() {
    let source = "
    class A {
        getName() {
            return this.name;
        }
    }
    class B < A {}
    class C < B {
        test() {
            return super.getName();
        }
    }
    var c = C();
    c.name = \"instancia C\";
    print c.test();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_extracted_method_using_super() {
    let source = "
    class Base {
        msg() {
            return \"base msg\";
        }
    }
    class Sub < Base {
        getSuperMsg() {
            return super.msg();
        }
    }
    var s = Sub();
    var fn_ptr = s.getSuperMsg;
    print fn_ptr();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_closure_captures_this_and_super() {
    let source = "
    class Base {
        greet() {
            return \"Base\";
        }
    }
    class Sub < Base {
        init(name) {
            this.name = name;
        }
        getGreeter() {
            fun greeter() {
                return super.greet() + \" -> \" + this.name;
            }
            return greeter;
        }
    }
    var s = Sub(\"Lox\");
    var g = s.getGreeter();
    print g();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_closure_uses_super_after_class_scope() {
    let source = "
    fun makeSub() {
        class Base {
            val() {
                return 10;
            }
        }
        class Sub < Base {
            val() {
                return super.val() + 5;
            }
        }
        return Sub();
    }
    var inst = makeSub();
    print inst.val();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_local_classes_with_inheritance() {
    let source = "
    {
        class Base {
            info() {
                return \"local base\";
            }
        }
        class Sub < Base {
            info() {
                return super.info() + \" & local sub\";
            }
        }
        print Sub().info();
    }
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_nested_classes_independent_contexts() {
    let source = "
    class Super {
        origin() {
            return \"Super\";
        }
    }
    class Outer < Super {
        getInner() {
            class Inner {
                info() {
                    return \"Inner standalone\";
                }
            }
            return Inner();
        }
        origin() {
            return super.origin() + \" -> Outer\";
        }
    }
    var o = Outer();
    print o.origin();
    print o.getInner().info();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_runtime_error_superclass_not_a_class() {
    let source = "
    var NotClass = \"cadena\";
    class Sub < NotClass {}
    ";
    assert_parity_err(source, "Superclass must be a class.");
}

#[test]
fn test_parity_runtime_error_undefined_super_property() {
    let source = "
    class Base {}
    class Sub < Base {
        callMissing() {
            super.noExiste();
        }
    }
    Sub().callMissing();
    ";
    assert_parity_err(source, "Undefined property 'noExiste'.");
}

// ===========================================================================
// E. Semántica y casos límite
// ===========================================================================

#[test]
fn test_parity_set_evaluation_order_lhs_checked_first() {
    let source = "
    fun sideEffect() {
        print \"EFECTO SECUNDARIO EJECUTADO\";
        return 123;
    }
    var x = \"no instancia\";
    x.campo = sideEffect();
    ";
    let tw = run_treewalk(source);
    let vm = run_vm(source);

    assert!(
        tw.error.is_some(),
        "TreeWalk debió fallar pero tuvo éxito. Salida:\n{}",
        tw.stdout
    );
    assert!(
        vm.error.is_some(),
        "VM debió fallar pero tuvo éxito. Salida:\n{}",
        vm.stdout
    );

    let tw_err = tw.error.unwrap();
    let vm_err = vm.error.unwrap();

    assert!(
        tw_err.contains("Only instances have fields."),
        "Error de TreeWalk ('{}') no contiene 'Only instances have fields.'",
        tw_err
    );
    assert!(
        vm_err.contains("Only instances have fields."),
        "Error de VM ('{}') no contiene 'Only instances have fields.'",
        vm_err
    );

    // Verificar que el RHS (sideEffect()) no se ejecutó en ninguno de los dos backends
    assert!(
        !tw.stdout.contains("EFECTO SECUNDARIO EJECUTADO"),
        "TreeWalk ejecutó indebidamente el valor asignado cuando el receptor no era instancia. Salida:\n{}",
        tw.stdout
    );
    assert!(
        !vm.stdout.contains("EFECTO SECUNDARIO EJECUTADO"),
        "VM ejecutó indebidamente el valor asignado cuando el receptor no era instancia. Salida:\n{}",
        vm.stdout
    );
    assert_eq!(tw.stdout, vm.stdout);
}

#[test]
fn test_parity_bound_methods_identity() {
    let source = "
    class Foo {
        m() {}
    }
    var f = Foo();
    var m1 = f.m;
    var m2 = f.m;
    print m1 == m1;
    print m1 == m2;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_same_name_in_different_scopes() {
    let source = "
    var x = \"global\";
    {
        var x = \"local\";
        print x;
    }
    print x;
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_shadowing_variables_and_classes() {
    let source = "
    class Thing {
        name() {
            return \"class Thing\";
        }
    }
    print Thing().name();
    {
        var Thing = \"shadowed\";
        print Thing;
    }
    print Thing().name();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_closures_outlive_enclosing_function() {
    let source = "
    fun makeCounter() {
        var count = 0;
        fun inc() {
            count = count + 1;
            return count;
        }
        return inc;
    }
    var c = makeCounter();
    print c();
    print c();
    print c();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_method_recursion() {
    let source = "
    class Math {
        fib(n) {
            if (n <= 1) return n;
            return this.fib(n - 1) + this.fib(n - 2);
        }
    }
    print Math().fib(8);
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_chained_calls() {
    let source = "
    class Node {
        init(val, next) {
            this.val = val;
            this.next = next;
        }
        getNext() {
            return this.next;
        }
        getVal() {
            return this.val;
        }
    }
    var list = Node(1, Node(2, Node(3, nil)));
    print list.getNext().getNext().getVal();
    ";
    assert_parity_ok(source);
}

#[test]
fn test_parity_runtime_error_method_wrong_arity() {
    let source = "
    class Foo {
        m(a, b) {}
    }
    Foo().m(1);
    ";
    assert_parity_err(source, "Expected 2 arguments but got 1.");
}

#[test]
fn test_parity_runtime_error_call_non_callable() {
    let source = "
    var notFn = 123;
    notFn();
    ";
    assert_parity_err(source, "Can only call functions and classes.");
}

// ===========================================================================
// F. Casos semánticos inválidos (Scanner / Parser / Checker compartidos)
// ===========================================================================

#[test]
fn test_parity_semantic_error_this_outside_class() {
    let source = "print this;";
    assert_parity_err(source, "Can't use 'this' outside of a class.");
}

#[test]
fn test_parity_semantic_error_super_outside_class() {
    let source = "print super.foo();";
    assert_parity_err(source, "Can't use 'super' outside of a class.");
}

#[test]
fn test_parity_semantic_error_super_without_superclass() {
    let source = "
    class NoSuper {
        foo() {
            super.foo();
        }
    }
    ";
    assert_parity_err(source, "Can't use 'super' in a class with no superclass.");
}

#[test]
fn test_parity_semantic_error_self_inheritance() {
    let source = "class Oops < Oops {}";
    assert_parity_err(source, "A class can't inherit from itself.");
}

#[test]
fn test_parity_semantic_error_return_value_from_init() {
    let source = "
    class Bad {
        init() {
            return \"valor prohibido\";
        }
    }
    ";
    assert_parity_err(source, "Can't return a value from an initializer.");
}
