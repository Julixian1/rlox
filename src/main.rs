use rlox::compiler;
use rlox::interpreter::Interpreter;
use rlox::parser::Parser;
use rlox::resolver::Resolver;
use rlox::scanner::Scanner;
use rlox::stmt::Stmt;
use rlox::vm::Vm;
use std::env;
use std::fs;
use std::io::{self, Write};

enum Backend {
    TreeWalk(Interpreter),
    Bytecode(Vm<io::Stdout>),
}

impl Backend {
    fn run(&mut self, source: String) {
        let (statements, resolver) = match parse_and_resolve(source) {
            Some(res) => res,
            None => return, 
        };

        match self {
            Backend::TreeWalk(interpreter) => {
                for (ptr, depth) in resolver.locals {
                    interpreter.resolve(ptr, depth);
                }

                if let Err(e) = interpreter.interpret(&statements) {
                    eprintln!("Runtime Error: {}", e);
                }
            }
            Backend::Bytecode(vm) => {
                let chunk = match compiler::compile(&statements) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Compile Error: {}", e);
                        return;
                    }
                };

                if env::var_os("RLOX_DISASSEMBLE").is_some() {
                    eprint!("{}", chunk.disassemble("script"));
                }

                if let Err(e) = vm.interpret(&chunk) {
                    eprintln!("Runtime Error: {}", e);
                }
            }
        }
    }
}

fn parse_and_resolve(source: String) -> Option<(Vec<Stmt>, Resolver)> {
    let mut scanner = Scanner::new(source);
    let tokens = match scanner.scan_tokens() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Scanning Error: {}", e);
            return None;
        }
    };

    let mut parser = Parser::new(tokens);
    let statements = match parser.parse() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Parse Error:\n{}", e);
            return None;
        }
    };

    let mut resolver = Resolver::new();
    resolver.resolve_stmts(&statements);

    if !resolver.errors.is_empty() {
        for err in &resolver.errors {
            eprintln!("{}", err);
        }
        return None;
    }

    Some((statements, resolver))
}

fn main() {
    let use_vm = env::args().skip(1).any(|a| a == "--vm");
    let args: Vec<String> = env::args().filter(|a| a != "--vm").collect();
    let mut backend = if use_vm {
        Backend::Bytecode(Vm::new())
    } else {
        Backend::TreeWalk(Interpreter::new())
    };

    if args.len() > 2 {
        println!("Uso: rlox [--vm] [script]");
        std::process::exit(64);
    } else if args.len() == 2 {
        let source = fs::read_to_string(&args[1]).expect("No se puede leer el archivo.");
        backend.run(source);
    } else {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            print!("> ");
            stdout.flush().unwrap();

            let mut line = String::new();
            if stdin.read_line(&mut line).unwrap() == 0 {
                break;
            }

            if line.trim().is_empty() {
                continue;
            }

            backend.run(line);
        }
    }
}