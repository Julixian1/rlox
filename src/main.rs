use rlox::checker;
use rlox::compiler::reg::{codegen, vm::RegVm};
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
use std::env;
use std::fs;
use std::io::{self, Write};

enum Backend {
    TreeWalk(Interpreter),
    Bytecode(Vm<io::Stdout>, OptConfig),
    Register(RegVm<io::Stdout>),
}

impl Backend {
    fn run(&mut self, source: String) {
        let statements = match parse_and_check(source) {
            Some(statements) => statements,
            None => return,
        };

        match self {
            Backend::TreeWalk(interpreter) => {
                let mut resolver = Resolver::new();
                resolver.resolve_stmts(&statements);
                for (ptr, depth) in resolver.locals {
                    interpreter.resolve(ptr, depth);
                }

                if let Err(e) = interpreter.interpret(&statements) {
                    eprintln!("Runtime Error: {}", e);
                }
            }
            Backend::Bytecode(vm, opt_config) => {
                let script = match compiler::compile_with_opt(&statements, opt_config.clone()) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Compile Error: {}", e);
                        return;
                    }
                };

                if env::var_os("RLOX_DISASSEMBLE").is_some() {
                    eprint!("{}", script.chunk.disassemble("script"));
                }

                if let Err(e) = vm.interpret(script) {
                    eprintln!("Runtime Error: {}", e);
                }
            }
            Backend::Register(vm) => {
                let script = match codegen::compile(&statements) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Compile Error: {}", e);
                        return;
                    }
                };

                if env::var_os("RLOX_DISASSEMBLE").is_some() {
                    eprint!("{}", script.chunk.disassemble("script"));
                }

                if let Err(e) = vm.interpret(script) {
                    eprintln!("Runtime Error: {}", e);
                }
            }
        }
    }
}

fn parse_and_check(source: String) -> Option<Vec<Stmt>> {
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

    let errors = checker::check(&statements);
    if !errors.is_empty() {
        for err in &errors {
            eprintln!("{}", err);
        }
        return None;
    }

    Some(statements)
}

fn main() {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    let mut use_vm = false;
    let mut use_reg = false;
    let mut opt_config = OptConfig::default(); // default -O0
    let mut opt_flags_specified = Vec::new();
    let mut positional_args = Vec::new();

    for arg in raw_args {
        match arg.as_str() {
            "--vm" => use_vm = true,
            "--reg" => use_reg = true,
            "-O0" => {
                opt_config = OptConfig::from_level(OptLevel::O0);
                opt_flags_specified.push(arg);
            }
            "-O1" => {
                opt_config = OptConfig::from_level(OptLevel::O1);
                opt_flags_specified.push(arg);
            }
            "-O2" => {
                opt_config = OptConfig::from_level(OptLevel::O2);
                opt_flags_specified.push(arg);
            }
            s if s.starts_with("--opt=") => {
                let passes = &s["--opt=".len()..];
                if let Err(e) = opt_config.apply_flags(passes, true) {
                    eprintln!("Error de argumentos: {}", e);
                    std::process::exit(64);
                }
                opt_flags_specified.push(arg);
            }
            s if s.starts_with("--no-opt=") => {
                let passes = &s["--no-opt=".len()..];
                if let Err(e) = opt_config.apply_flags(passes, false) {
                    eprintln!("Error de argumentos: {}", e);
                    std::process::exit(64);
                }
                opt_flags_specified.push(arg);
            }
            flag if flag.starts_with('-') => {
                eprintln!("Error: Flag desconocido '{}'.", flag);
                std::process::exit(64);
            }
            _ => positional_args.push(arg),
        }
    }

    if use_vm && use_reg {
        eprintln!("Error: No se pueden especificar --vm y --reg simultáneamente.");
        std::process::exit(64);
    }

    if !use_vm && !opt_flags_specified.is_empty() {
        let backend_name = if use_reg {
            "VM de registros (--reg)"
        } else {
            "Tree-Walk"
        };
        eprintln!(
            "Error: Los flags de optimización ({}) solo son válidos con la VM de pila (--vm). No están soportados en el backend {}.",
            opt_flags_specified.join(", "),
            backend_name
        );
        std::process::exit(64);
    }

    let mut backend = if use_reg {
        Backend::Register(RegVm::new())
    } else if use_vm {
        Backend::Bytecode(Vm::new(), opt_config)
    } else {
        Backend::TreeWalk(Interpreter::new())
    };

    if positional_args.len() > 1 {
        println!("Uso: rlox [--vm [-O0|-O1|-O2] [--opt=...] [--no-opt=...] | --reg] [script]");
        std::process::exit(64);
    } else if positional_args.len() == 1 {
        let source = fs::read_to_string(&positional_args[0]).expect("No se puede leer el archivo.");
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
