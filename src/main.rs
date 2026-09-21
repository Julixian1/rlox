mod expr;
mod parser;
mod scanner;
mod stmt;
mod token;

use parser::Parser;
use scanner::Scanner;
use std::env;
use std::fs;
use std::io::{self, Write};

fn run(source: String) {
    let mut scanner = Scanner::new(source);
    let tokens = match scanner.scan_tokens() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Scanning Error: {}", e);
            return;
        }
    };

    let mut parser = Parser::new(tokens);
    match parser.parse() {
        Ok(statements) => {
            for stmt in &statements {
                println!("{:?}", stmt);
            }
        }
        Err(e) => eprintln!("Parse Error:\n{}", e),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 2 {
        println!("Uso: rlox [script]");
        std::process::exit(64);
    } else if args.len() == 2 {
        let source = fs::read_to_string(&args[1]).expect("No se puede leer el archivo.");
        run(source);
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

            run(line);
        }
    }
}

