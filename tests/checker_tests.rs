use rlox::checker;
use rlox::parser::Parser;
use rlox::scanner::Scanner;

fn check_source(source: &str) -> Vec<String> {
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens().unwrap();
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().unwrap();
    checker::check(&statements)
}

// ---- variables duplicadas ---------------------------------------------------

#[test]
fn test_checker_error_redefining_variable_in_same_scope() {
    let errors = check_source(
        "
    {
      var a = 1;
      var a = 2;
    }
    ",
    );

    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Already a variable with this name in this scope."));
}

#[test]
fn test_checker_allows_redeclaring_globals() {
    assert!(check_source("var a = 1; var a = 2;").is_empty());
}

#[test]
fn test_checker_allows_same_name_in_different_scopes() {
    assert!(check_source("{ var a = 1; { var a = 2; } } { var a = 3; }").is_empty());
}

#[test]
fn test_checker_parameter_and_body_share_one_scope() {
    let errors = check_source("fun f(a) { var a = 1; }");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Already a variable with this name in this scope."));
}

#[test]
fn test_checker_duplicate_parameters() {
    let errors = check_source("fun f(a, a) {}");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Already a variable with this name in this scope."));
}

// ---- lectura en el propio inicializador -------------------------------------

#[test]
fn test_checker_error_read_in_own_initializer() {
    let errors = check_source(
        "
    {
      var a = a;
    }
    ",
    );

    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Can't read local variable in its own initializer."));
}

#[test]
fn test_checker_own_initializer_inside_a_larger_expression() {
    let errors = check_source("{ var a = 1 + (2 * a); }");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Can't read local variable in its own initializer."));
}

#[test]
fn test_checker_global_can_read_itself_in_initializer() {
    // Los globales se resuelven en runtime: no hay scope local que lo impida.
    assert!(check_source("var a = a;").is_empty());
}

#[test]
fn test_checker_reading_another_variable_in_initializer_is_fine() {
    assert!(check_source("{ var a = 1; { var b = a; } }").is_empty());
}

#[test]
fn test_checker_function_can_call_itself() {
    assert!(check_source("{ fun f() { f(); } }").is_empty());
}

// ---- return ----------------------------------------------------------------

#[test]
fn test_checker_error_return_from_top_level() {
    let errors = check_source("return \"at top level\";");

    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Can't return from top-level code."));
}

#[test]
fn test_checker_return_inside_top_level_block_is_still_top_level() {
    let errors = check_source("{ return 1; }");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Can't return from top-level code."));
}

#[test]
fn test_checker_return_inside_top_level_if_and_while() {
    assert_eq!(check_source("if (true) return 1;").len(), 1);
    assert_eq!(check_source("while (true) return 1;").len(), 1);
}

#[test]
fn test_checker_return_inside_function_is_fine() {
    assert!(check_source("fun f() { return 1; }").is_empty());
}

#[test]
fn test_checker_return_nested_in_blocks_loops_and_functions_is_fine() {
    let source = "
    fun outer() {
      while (true) {
        if (true) { return 1; }
      }
      fun inner() { return 2; }
      return inner;
    }";
    assert!(check_source(source).is_empty());
}

#[test]
fn test_checker_return_after_a_function_is_top_level_again() {
    let errors = check_source("fun f() { return 1; } return 2;");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Can't return from top-level code."));
}

// ---- varias pasadas juntas ---------------------------------------------------

#[test]
fn test_checker_reports_errors_from_every_pass() {
    let errors = check_source("{ var a = 1; var a = 2; }\nreturn 3;");
    assert_eq!(errors.len(), 2);
}

#[test]
fn test_checker_orders_errors_by_line_not_by_pass() {
    let source = "return 1;\n{\n  var a = 1;\n  var a = 2;\n}\nreturn 2;";
    let errors = check_source(source);

    assert_eq!(errors.len(), 3);
    assert!(errors[0].starts_with("[line 1]"), "{errors:?}");
    assert!(errors[1].starts_with("[line 4]"), "{errors:?}");
    assert!(errors[2].starts_with("[line 6]"), "{errors:?}");
}

#[test]
fn test_checker_valid_program_has_no_errors() {
    let source = "
    var a = \"global\";
    fun make() {
      var i = 0;
      fun count() { i = i + 1; return i; }
      return count;
    }
    { var b = a; print b; }
    ";
    assert!(check_source(source).is_empty());
}

#[test]
fn test_checker_error_this_outside_class() {
    let top_level = check_source("print this;");
    assert_eq!(top_level.len(), 1);
    assert!(top_level[0].contains("Can't use 'this' outside of a class."));

    let inside_fn = check_source("fun foo() { print this; }");
    assert_eq!(inside_fn.len(), 1);
    assert!(inside_fn[0].contains("Can't use 'this' outside of a class."));
}

#[test]
fn test_checker_valid_this_in_methods_and_nested_classes() {
    let source = "
    class Foo {
      bar() {
        print this;
      }
    }
    class Outer {
      outer_method() {
        class Inner {
          inner_method() {
            print this;
          }
        }
        print this;
      }
    }
    ";
    assert!(check_source(source).is_empty());
}

#[test]
fn test_checker_error_return_value_from_init() {
    let errors = check_source("class Foo { init() { return 5; } }");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Can't return a value from an initializer."));
}

#[test]
fn test_checker_allow_empty_return_in_init() {
    let errors = check_source("class Foo { init() { return; } }");
    assert!(errors.is_empty());
}

#[test]
fn test_checker_allow_return_value_in_function_inside_init() {
    let source = "
    class Foo {
      init() {
        fun helper() {
          return 42;
        }
        print helper();
      }
    }
    ";
    assert!(check_source(source).is_empty());
}

#[test]
fn test_checker_error_super_outside_class() {
    let errors = check_source("super.method();");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Can't use 'super' outside of a class."));
}

#[test]
fn test_checker_error_super_in_class_without_superclass() {
    let errors = check_source("class Base { foo() { super.bar(); } }");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Can't use 'super' in a class with no superclass."));
}

#[test]
fn test_checker_valid_super_in_subclass() {
    let source = "
    class Base {
      bar() {}
    }
    class Sub < Base {
      foo() {
        super.bar();
      }
    }
    ";
    assert!(check_source(source).is_empty());
}

#[test]
fn test_checker_nested_classes_preserve_semantic_contexts() {
    let source = "
    class Base {
      baseMethod() {}
    }
    class Outer < Base {
      outerMethod() {
        super.baseMethod();
        class Inner {
          innerMethod() {
            super.baseMethod();
          }
        }
      }
    }
    ";
    let errors = check_source(source);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Can't use 'super' in a class with no superclass."));
}

#[test]
fn test_checker_nested_classes_preserve_semantic_contexts() {
    let source = "
    class Base {
      baseMethod() {}
    }
    class Outer < Base {
      outerMethod() {
        super.baseMethod();
        class Inner {
          innerMethod() {
            super.baseMethod();
          }
        }
      }
    }
    ";
    let errors = check_source(source);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Can't use 'super' in a class with no superclass."));
}
