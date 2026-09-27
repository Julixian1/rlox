use rlox::interpreter::Interpreter;
use rlox::parser::Parser;
use rlox::resolver::Resolver;
use rlox::scanner::Scanner;
use std::env;
use std::fs;
use std::io::{self, Write};

/// Ejecuta un fragmento o archivo de código fuente en Lox.
///
/// Escanea el código fuente a tokens, parsea a sentencias del AST,
/// resuelve los alcances de variables locales y finalmente ejecuta el programa con el interprete.
fn run(interpreter: &mut Interpreter, source: String) {

    let mut scanner = Scanner::new(source);
    let tokens = match scanner.scan_tokens() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Scanning Error: {}", e);
            return;
        }
    };

    let mut parser = Parser::new(tokens);
    let statements = match parser.parse() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Parse Error:\n{}", e);
            return;
        }
    };

    let mut resolver = Resolver::new();
    resolver.resolve_stmts(&statements);

    if !resolver.errors.is_empty() {
        for err in &resolver.errors {
            eprintln!("{}", err);
        }
        return;
    }

    // Transfiere el mapa de profundidades al Interpreter.
    for (ptr, depth) in resolver.locals {
        interpreter.resolve(ptr, depth);
    }

    if let Err(e) = interpreter.interpret(&statements) {
        eprintln!("Runtime Error: {}", e);
    }
}

/// Punto de entrada principal del ejecutable CLI de RLox.
///
/// Soporta dos modos:
/// 1. Ejecución de un archivo de script (`rlox <script>`).
/// 2. Modo interactivo REPL (`rlox`).
fn main() {
    let args: Vec<String> = env::args().collect();
    let mut interpreter = Interpreter::new();

    if args.len() > 2 {
        println!("Uso: rlox [script]");
        std::process::exit(64);
    } else if args.len() == 2 {
        let source = fs::read_to_string(&args[1]).expect("No se puede leer el archivo.");
        run(&mut interpreter, source);
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

            run(&mut interpreter, line);
        }
    }
}
