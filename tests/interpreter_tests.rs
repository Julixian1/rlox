use rlox::checker;
use rlox::interpreter::Interpreter;
use rlox::parser::Parser;
use rlox::resolver::Resolver;
use rlox::scanner::Scanner;
use rlox::token::{Token, TokenType};
use rlox::value::Value;

fn execute_source(interpreter: &mut Interpreter, source: &str) -> Result<(), String> {
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens()?;
    let mut parser = Parser::new(tokens);
    let statements = parser.parse()?;

    let errors = checker::check(&statements);
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    let mut resolver = Resolver::new();
    resolver.resolve_stmts(&statements);

    for (ptr, depth) in resolver.locals {
        interpreter.resolve(ptr, depth);
    }

    interpreter.interpret(&statements)
}

#[test]
fn test_interpreter_arithmetic_and_string_concatenation() {
    let mut interpreter = Interpreter::new();
    let source = "
    var num = (10 + 5) * 2 % 7;
    var str = \"Hello \" + \"World\";
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
}

#[test]
fn test_interpreter_variable_scoping() {
    let mut interpreter = Interpreter::new();
    let source = "
    var a = \"global\";
    {
      var a = \"local\";
      print a;
    }
    print a;
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
}

#[test]
fn test_interpreter_control_flow_while_and_if() {
    let mut interpreter = Interpreter::new();
    let source = "
    var count = 0;
    while (count < 5) {
      if (count % 2 == 0) {
        print count;
      }
      count = count + 1;
    }
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
}

#[test]
fn test_interpreter_function_call_and_return() {
    let mut interpreter = Interpreter::new();
    let source = "
    fun fib(n) {
      if (n <= 1) return n;
      return fib(n - 1) + fib(n - 2);
    }
    var res = fib(10);
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
}

#[test]
fn test_interpreter_closure_binding() {
    let mut interpreter = Interpreter::new();
    let source = "
    fun makeCounter() {
      var i = 0;
      fun count() {
        i = i + 1;
        return i;
      }
      return count;
    }

    var counter = makeCounter();
    counter();
    counter();
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
}

#[test]
fn test_interpreter_runtime_error_type_mismatch() {
    let mut interpreter = Interpreter::new();
    let source = "var result = \"texto\" - 5;";
    let result = execute_source(&mut interpreter, source);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Operands must be numbers."));
}

fn get_global(interpreter: &Interpreter, name: &str) -> Value {
    interpreter
        .globals
        .borrow()
        .get(&Token::new(
            TokenType::Identifier,
            name.to_string(),
            None,
            0,
        ))
        .unwrap()
}

#[test]
fn test_interpreter_class_instantiation() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Bagel {}
    var bagel = Bagel();
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    let bagel = get_global(&interpreter, "bagel");
    assert!(matches!(bagel, Value::Instance(_)));
}

#[test]
fn test_interpreter_field_access_and_set() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Box {}
    var box1 = Box();
    var box2 = Box();
    box1.value = 10;
    box2.value = 20;
    var v1 = box1.value;
    var v2 = box2.value;
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(get_global(&interpreter, "v1"), Value::Number(10.0));
    assert_eq!(get_global(&interpreter, "v2"), Value::Number(20.0));
}

#[test]
fn test_interpreter_method_call_and_this_binding() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Counter {
      init() {
        this.count = 0;
      }
      increment() {
        this.count = this.count + 1;
        return this.count;
      }
    }
    var c = Counter();
    var first = c.increment();
    var second = c.increment();
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(get_global(&interpreter, "first"), Value::Number(1.0));
    assert_eq!(get_global(&interpreter, "second"), Value::Number(2.0));
}

#[test]
fn test_interpreter_init_with_arguments_and_implicit_return() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Point {
      init(x, y) {
        this.x = x;
        this.y = y;
      }
    }
    var p = Point(3, 4);
    var px = p.x;
    var py = p.y;
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(get_global(&interpreter, "px"), Value::Number(3.0));
    assert_eq!(get_global(&interpreter, "py"), Value::Number(4.0));
}

#[test]
fn test_interpreter_method_inheritance() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Animal {
      speak() {
        return \"sound\";
      }
    }
    class Dog < Animal {}
    var dog = Dog();
    var result = dog.speak();
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(
        get_global(&interpreter, "result"),
        Value::String("sound".to_string())
    );
}

#[test]
fn test_interpreter_method_override() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Animal {
      speak() {
        return \"sound\";
      }
    }
    class Dog < Animal {
      speak() {
        return \"woof\";
      }
    }
    var dog = Dog();
    var result = dog.speak();
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(
        get_global(&interpreter, "result"),
        Value::String("woof".to_string())
    );
}

#[test]
fn test_interpreter_super_method_call() {
    let mut interpreter = Interpreter::new();
    let source = "
    class A {
      method() {
        return \"A\";
      }
    }
    class B < A {
      method() {
        return \"B \" + super.method();
      }
    }
    var b = B();
    var result = b.method();
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(
        get_global(&interpreter, "result"),
        Value::String("B A".to_string())
    );
}

#[test]
fn test_interpreter_three_level_hierarchy_super() {
    let mut interpreter = Interpreter::new();
    let source = "
    class A {
      test() {
        return \"A\";
      }
    }
    class B < A {
      test() {
        return \"B \" + super.test();
      }
    }
    class C < B {
      test() {
        return \"C \" + super.test();
      }
    }
    var c = C();
    var result = c.test();
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(
        get_global(&interpreter, "result"),
        Value::String("C B A".to_string())
    );
}

#[test]
fn test_interpreter_inherited_initializer() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Parent {
      init(name) {
        this.name = name;
      }
    }
    class Child < Parent {}
    var child = Child(\"Alex\");
    var res_name = child.name;
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(
        get_global(&interpreter, "res_name"),
        Value::String("Alex".to_string())
    );
}

#[test]
fn test_interpreter_runtime_error_superclass_not_a_class() {
    let mut interpreter = Interpreter::new();
    let source = "
    var NotClass = \"I am a string\";
    class Sub < NotClass {}
    ";
    let result = execute_source(&mut interpreter, source);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Superclass must be a class."));
}

#[test]
fn test_interpreter_runtime_error_undefined_property() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Foo {}
    var foo = Foo();
    foo.non_existent;
    ";
    let result = execute_source(&mut interpreter, source);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Undefined property 'non_existent'.")
    );
}

#[test]
fn test_interpreter_identity_equality() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Foo {}
    var a = Foo();
    var b = Foo();
    var c = a;
    var eq_self = (a == a);
    var eq_alias = (a == c);
    var eq_diff = (a == b);
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(get_global(&interpreter, "eq_self"), Value::Boolean(true));
    assert_eq!(get_global(&interpreter, "eq_alias"), Value::Boolean(true));
    assert_eq!(get_global(&interpreter, "eq_diff"), Value::Boolean(false));
}

#[test]
fn test_interpreter_initializer_propagates_runtime_error() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Foo {
      init() {
        var x = \"bad\" - 5;
      }
    }
    Foo();
    ";
    let result = execute_source(&mut interpreter, source);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Operands must be numbers."));
}

#[test]
fn test_interpreter_extracted_method_retains_this() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Person {
      init(name) {
        this.name = name;
      }
      sayName() {
        return this.name;
      }
    }
    var person = Person(\"Bob\");
    var fn_ptr = person.sayName;
    var name = fn_ptr();
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(
        get_global(&interpreter, "name"),
        Value::String("Bob".to_string())
    );
}

#[test]
fn test_interpreter_field_shadows_method() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Foo {
      bar() {
        return \"method\";
      }
    }
    var foo = Foo();
    foo.bar = \"field\";
    var res = foo.bar;
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(
        get_global(&interpreter, "res"),
        Value::String("field".to_string())
    );
}

#[test]
fn test_interpreter_three_level_super_maintains_original_receiver() {
    let mut interpreter = Interpreter::new();
    let source = "
    class A {
      getName() {
        return this.name;
      }
    }
    class B < A {}
    class C < B {
      init(name) {
        this.name = name;
      }
      test() {
        return super.getName();
      }
    }
    var c = C(\"Charlie\");
    var res = c.test();
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(
        get_global(&interpreter, "res"),
        Value::String("Charlie".to_string())
    );
}

#[test]
fn test_interpreter_this_and_super_in_nested_functions() {
    let mut interpreter = Interpreter::new();
    let source = "
    class Base {
      greet() {
        return \"hello\";
      }
    }
    class Derived < Base {
      init(name) {
        this.name = name;
      }
      getGreeter() {
        fun nested() {
          return super.greet() + \" \" + this.name;
        }
        return nested;
      }
    }
    var d = Derived(\"World\");
    var greeter = d.getGreeter();
    var res = greeter();
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(
        get_global(&interpreter, "res"),
        Value::String("hello World".to_string())
    );
}

#[test]
fn test_interpreter_functions_compared_by_identity() {
    let mut interpreter = Interpreter::new();
    let source = "
    fun f() {}
    fun g() {}
    var eq_diff = (f == g);

    fun make() {
      fun inner() {}
      return inner;
    }
    var fn1 = make();
    var fn2 = make();
    var eq_closures = (fn1 == fn2);
    var eq_same = (fn1 == fn1);
    ";
    assert!(execute_source(&mut interpreter, source).is_ok());
    assert_eq!(get_global(&interpreter, "eq_diff"), Value::Boolean(false));
    assert_eq!(
        get_global(&interpreter, "eq_closures"),
        Value::Boolean(false)
    );
    assert_eq!(get_global(&interpreter, "eq_same"), Value::Boolean(true));
}
