use rlox::interpreter::Interpreter;
use rlox::parser::Parser;
use rlox::resolver::Resolver;
use rlox::scanner::Scanner;

fn execute_source(interpreter: &mut Interpreter, source: &str) -> Result<(), String> {
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens()?;
    let mut parser = Parser::new(tokens);
    let statements = parser.parse()?;

    let mut resolver = Resolver::new();
    resolver.resolve_stmts(&statements);

    if !resolver.errors.is_empty() {
        return Err(resolver.errors.join("\n"));
    }

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
