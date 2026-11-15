//! Harness de benchmarks para `rlox`.
//!
//! Permite medir de forma separada:
//! - Frontend (Scanner + Parser + Checker)
//! - Compilación a Bytecode
//! - Ejecución en VM (capturando stdout en un buffer en memoria)
//! - Métricas de bytecode: cantidad de instrucciones, constantes y bytes aproximados.
//!
//! Permite calcular mediana, IQR, y exportar a formato CSV y consola tabular.

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

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Backend objetivo para las mediciones del benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetBackend {
    /// Máquina virtual de pila (Stack VM).
    Vm,
    /// Máquina virtual de registros (Register VM).
    Reg,
    /// Intérprete Tree-Walk sobre AST.
    Tree,
    /// Todos los backends soportados secuencialmente.
    All,
}

/// Parámetros de configuración de la corrida de benchmarks.
#[derive(Debug, Clone)]
struct BenchConfig {
    /// Cantidad de ejecuciones de precalentamiento (warmup) no cronometradas.
    warmup: usize,
    /// Cantidad de repeticiones medidas para el cálculo estadístico.
    reps: usize,
    /// Backend o conjunto de backends a evaluar.
    backend: TargetBackend,
    /// Configuración de optimizaciones para la Stack VM.
    opt_config: OptConfig,
    /// Ruta opcional del archivo CSV de salida.
    csv_path: Option<PathBuf>,
    /// Archivos `.lox` a evaluar.
    files: Vec<PathBuf>,
}

/// Resumen estadístico de una serie de tiempos de ejecución (mediana, rango intercuartílico, min y max).
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct StatSummary {
    /// Mediana en milisegundos.
    median_ms: f64,
    /// Rango intercuartílico (IQR) en milisegundos.
    iqr_ms: f64,
    /// Tiempo mínimo en milisegundos.
    min_ms: f64,
    /// Tiempo máximo en milisegundos.
    max_ms: f64,
}

impl StatSummary {
    /// Calcula el resumen estadístico a partir de un vector de duraciones.
    fn from_durations(mut v: Vec<Duration>) -> Self {
        if v.is_empty() {
            return Self {
                median_ms: 0.0,
                iqr_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
            };
        }
        v.sort();
        let ms: Vec<f64> = v.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
        let median = calc_median(&ms);
        let q1 = calc_quantile(&ms, 0.25);
        let q3 = calc_quantile(&ms, 0.75);
        let iqr = q3 - q1;
        Self {
            median_ms: median,
            iqr_ms: iqr,
            min_ms: ms[0],
            max_ms: ms[ms.len() - 1],
        }
    }
}

/// Calcula la mediana (cuantil 0.50) de una serie de valores en coma flotante.
fn calc_median(v: &[f64]) -> f64 {
    calc_quantile(v, 0.50)
}

/// Calcula un cuantil arbitrario (0.0 a 1.0) mediante interpolación lineal.
fn calc_quantile(v: &[f64], q: f64) -> f64 {
    let n = v.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return v[0];
    }
    let pos = (n - 1) as f64 * q;
    let base = pos.floor() as usize;
    let rest = pos - base as f64;
    if base + 1 < n {
        v[base] + rest * (v[base + 1] - v[base])
    } else {
        v[base]
    }
}

/// Métricas cuantitativas del bytecode compilado.
struct BytecodeStats {
    /// Cantidad total de instrucciones de bytecode.
    instructions: usize,
    /// Cantidad total de constantes en el pool de constantes.
    constants: usize,
    /// Tamaño estimado en bytes de la representación compilada.
    approx_bytes: usize,
}

/// Recorre recursivamente un prototipo de función de la Stack VM y calcula las métricas de bytecode acumuladas.
fn collect_vm_bytecode_stats(proto: &Rc<rlox::closure::FnProto>) -> BytecodeStats {
    let mut visited: HashSet<*const rlox::closure::FnProto> = HashSet::new();
    let mut total_instructions = 0;
    let mut total_constants = 0;
    let mut total_bytes = 0;

    fn traverse(
        proto: &Rc<rlox::closure::FnProto>,
        visited: &mut HashSet<*const rlox::closure::FnProto>,
        instructions: &mut usize,
        constants: &mut usize,
        bytes: &mut usize,
    ) {
        let ptr = Rc::as_ptr(proto);
        if !visited.insert(ptr) {
            return;
        }

        let chunk = &proto.chunk;
        *instructions += chunk.code.len();
        *constants += chunk.constants.len();
        *bytes += chunk.code.len() * std::mem::size_of::<rlox::chunk::OpCode>()
            + chunk.constants.len() * std::mem::size_of::<rlox::value::Value>()
            + chunk.lines.len() * std::mem::size_of::<usize>();

        for c in &chunk.constants {
            if let rlox::value::Value::Proto(nested) = c {
                traverse(nested, visited, instructions, constants, bytes);
            }
        }
    }

    traverse(
        proto,
        &mut visited,
        &mut total_instructions,
        &mut total_constants,
        &mut total_bytes,
    );

    BytecodeStats {
        instructions: total_instructions,
        constants: total_constants,
        approx_bytes: total_bytes,
    }
}

/// Recorre recursivamente un prototipo de función de la Register VM y calcula las métricas de bytecode acumuladas.
fn collect_reg_bytecode_stats(proto: &Rc<rlox::compiler::reg::closure::RegProto>) -> BytecodeStats {
    let mut visited: HashSet<*const rlox::compiler::reg::closure::RegProto> = HashSet::new();
    let mut total_instructions = 0;
    let mut total_constants = 0;
    let mut total_bytes = 0;

    fn traverse(
        proto: &Rc<rlox::compiler::reg::closure::RegProto>,
        visited: &mut HashSet<*const rlox::compiler::reg::closure::RegProto>,
        instructions: &mut usize,
        constants: &mut usize,
        bytes: &mut usize,
    ) {
        let ptr = Rc::as_ptr(proto);
        if !visited.insert(ptr) {
            return;
        }

        let chunk = &proto.chunk;
        *instructions += chunk.code.len();
        *constants += chunk.constants.len();
        *bytes += chunk.code.len() * std::mem::size_of::<rlox::compiler::reg::chunk::RegOp>()
            + chunk.constants.len() * std::mem::size_of::<rlox::value::Value>()
            + chunk.lines.len() * std::mem::size_of::<usize>();

        for c in &chunk.constants {
            if let rlox::value::Value::RegProto(nested) = c {
                traverse(nested, visited, instructions, constants, bytes);
            }
        }
    }

    traverse(
        proto,
        &mut visited,
        &mut total_instructions,
        &mut total_constants,
        &mut total_bytes,
    );

    BytecodeStats {
        instructions: total_instructions,
        constants: total_constants,
        approx_bytes: total_bytes,
    }
}

/// Parsea los argumentos de línea de comandos para construir la configuración del benchmark.
fn parse_args() -> BenchConfig {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut warmup = 3;
    let mut reps = 10;
    let mut backend = TargetBackend::Vm;
    let mut opt_config = OptConfig::default();
    let mut csv_path = None;
    let mut files = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--warmup" => {
                i += 1;
                warmup = args[i].parse().expect("warmup debe ser un entero positivo");
            }
            "--reps" => {
                i += 1;
                reps = args[i].parse().expect("reps debe ser un entero positivo");
            }
            "--backend" => {
                i += 1;
                backend = match args[i].as_str() {
                    "vm" => TargetBackend::Vm,
                    "reg" => TargetBackend::Reg,
                    "tree" => TargetBackend::Tree,
                    "all" => TargetBackend::All,
                    other => panic!("Backend desconocido: {other}. Opciones: vm, reg, tree, all"),
                };
            }
            "-O0" | "--opt-level=0" => {
                opt_config = OptConfig::from_level(OptLevel::O0);
            }
            "-O1" | "--opt-level=1" => {
                opt_config = OptConfig::from_level(OptLevel::O1);
            }
            "-O2" | "--opt-level=2" => {
                opt_config = OptConfig::from_level(OptLevel::O2);
            }
            "--opt-level" => {
                i += 1;
                let lvl =
                    OptLevel::parse(&args[i]).expect("nivel de optimización inválido (0, 1, 2)");
                opt_config = OptConfig::from_level(lvl);
            }
            s if s.starts_with("--opt=") => {
                let passes = &s["--opt=".len()..];
                opt_config
                    .apply_flags(passes, true)
                    .expect("flags de optimización inválidos");
            }
            s if s.starts_with("--no-opt=") => {
                let passes = &s["--no-opt=".len()..];
                opt_config
                    .apply_flags(passes, false)
                    .expect("flags de optimización inválidos");
            }
            "--csv" => {
                i += 1;
                csv_path = Some(PathBuf::from(&args[i]));
            }
            flag if flag.starts_with('-') => {
                panic!("Flag desconocido: {flag}");
            }
            path => {
                files.push(PathBuf::from(path));
            }
        }
        i += 1;
    }

    if files.is_empty() {
        let default_dir = Path::new("benchmarks").join("programs");
        if default_dir.exists() {
            for entry in fs::read_dir(&default_dir).expect("leer benchmarks/programs") {
                let p = entry.expect("archivo").path();
                if p.extension().is_some_and(|e| e == "lox") {
                    files.push(p);
                }
            }
            files.sort();
        } else {
            panic!("No se especificaron archivos .lox ni existe benchmarks/programs/");
        }
    }

    BenchConfig {
        warmup,
        reps,
        backend,
        opt_config,
        csv_path,
        files,
    }
}

/// Ejecuta las etapas del frontend (escaneo, parseo y chequeo) sobre el código fuente Lox.
fn run_frontend(source: &str) -> Result<Vec<Stmt>, String> {
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

/// Ejecuta mediciones detalladas de frontend, compilación y tiempo de VM en la Stack VM.
fn bench_vm_program(
    source: &str,
    warmup: usize,
    reps: usize,
    opt: OptConfig,
) -> Result<
    (
        StatSummary,
        StatSummary,
        StatSummary,
        StatSummary,
        BytecodeStats,
        String,
    ),
    String,
> {
    for _ in 0..warmup {
        let stmts = run_frontend(source)?;
        let proto = compiler::compile_with_opt(&stmts, opt.clone())?;
        let mut out = Vec::new();
        let mut vm = Vm::with_output(&mut out);
        vm.interpret(proto)?;
    }

    let mut t_front = Vec::with_capacity(reps);
    let mut t_compile = Vec::with_capacity(reps);
    let mut t_vm = Vec::with_capacity(reps);
    let mut t_total = Vec::with_capacity(reps);
    let mut last_output = String::new();
    let mut stats = BytecodeStats {
        instructions: 0,
        constants: 0,
        approx_bytes: 0,
    };

    for _ in 0..reps {
        let start_total = Instant::now();

        let start_front = Instant::now();
        let stmts = run_frontend(source)?;
        let d_front = start_front.elapsed();

        let start_compile = Instant::now();
        let proto = compiler::compile_with_opt(&stmts, opt.clone())?;
        let d_compile = start_compile.elapsed();

        let mut out = Vec::new();
        let start_vm = Instant::now();
        {
            let mut vm = Vm::with_output(&mut out);
            vm.interpret(proto.clone())?;
        }
        let d_vm = start_vm.elapsed();
        let d_total = start_total.elapsed();

        t_front.push(d_front);
        t_compile.push(d_compile);
        t_vm.push(d_vm);
        t_total.push(d_total);

        stats = collect_vm_bytecode_stats(&proto);

        last_output = String::from_utf8(out).unwrap_or_default();
    }

    Ok((
        StatSummary::from_durations(t_front),
        StatSummary::from_durations(t_compile),
        StatSummary::from_durations(t_vm),
        StatSummary::from_durations(t_total),
        stats,
        last_output,
    ))
}

/// Ejecuta mediciones detalladas de frontend, compilación y tiempo de VM en la Register VM.
fn bench_reg_program(
    source: &str,
    warmup: usize,
    reps: usize,
) -> Result<
    (
        StatSummary,
        StatSummary,
        StatSummary,
        StatSummary,
        BytecodeStats,
        String,
    ),
    String,
> {
    for _ in 0..warmup {
        let stmts = run_frontend(source)?;
        let proto = codegen::compile(&stmts)?;
        let mut out = Vec::new();
        let mut vm = RegVm::with_output(&mut out);
        vm.interpret(proto)?;
    }

    let mut t_front = Vec::with_capacity(reps);
    let mut t_compile = Vec::with_capacity(reps);
    let mut t_vm = Vec::with_capacity(reps);
    let mut t_total = Vec::with_capacity(reps);
    let mut last_output = String::new();
    let mut stats = BytecodeStats {
        instructions: 0,
        constants: 0,
        approx_bytes: 0,
    };

    for _ in 0..reps {
        let start_total = Instant::now();

        let start_front = Instant::now();
        let stmts = run_frontend(source)?;
        let d_front = start_front.elapsed();

        let start_compile = Instant::now();
        let proto = codegen::compile(&stmts)?;
        let d_compile = start_compile.elapsed();

        let mut out = Vec::new();
        let start_vm = Instant::now();
        {
            let mut vm = RegVm::with_output(&mut out);
            vm.interpret(proto.clone())?;
        }
        let d_vm = start_vm.elapsed();
        let d_total = start_total.elapsed();

        t_front.push(d_front);
        t_compile.push(d_compile);
        t_vm.push(d_vm);
        t_total.push(d_total);

        stats = collect_reg_bytecode_stats(&proto);

        last_output = String::from_utf8(out).unwrap_or_default();
    }

    Ok((
        StatSummary::from_durations(t_front),
        StatSummary::from_durations(t_compile),
        StatSummary::from_durations(t_vm),
        StatSummary::from_durations(t_total),
        stats,
        last_output,
    ))
}

/// Ejecuta mediciones de tiempo de frontend y de interpretación en el backend Tree-Walk.
fn bench_tree_program(
    source: &str,
    warmup: usize,
    reps: usize,
) -> Result<(StatSummary, StatSummary, String), String> {
    for _ in 0..warmup {
        let stmts = run_frontend(source)?;
        let mut resolver = Resolver::new();
        resolver.resolve_stmts(&stmts);
        let mut interpreter = Interpreter::new();
        for (ptr, depth) in resolver.locals {
            interpreter.resolve(ptr, depth);
        }
        let _ = interpreter.interpret(&stmts);
    }

    let mut t_front = Vec::with_capacity(reps);
    let mut t_run = Vec::with_capacity(reps);
    let last_output = String::new();

    for _ in 0..reps {
        let start_front = Instant::now();
        let stmts = run_frontend(source)?;
        let d_front = start_front.elapsed();

        let start_run = Instant::now();
        let mut resolver = Resolver::new();
        resolver.resolve_stmts(&stmts);
        let mut interpreter = Interpreter::new();
        for (ptr, depth) in resolver.locals {
            interpreter.resolve(ptr, depth);
        }
        let _ = interpreter.interpret(&stmts);
        let d_run = start_run.elapsed();

        t_front.push(d_front);
        t_run.push(d_run);
    }

    Ok((
        StatSummary::from_durations(t_front),
        StatSummary::from_durations(t_run),
        last_output,
    ))
}

/// Registro individual de métricas de benchmark exportable a formato tabular CSV.
struct CsvRecord {
    /// Nombre identificador del benchmark.
    benchmark: String,
    /// Backend evaluado ("vm", "reg", "tree").
    backend: String,
    /// Nivel de optimización aplicado.
    opt_level: String,
    /// Pases de optimización activos.
    opt_passes: String,
    /// Cantidad de repeticiones ejecutadas.
    reps: usize,
    /// Cantidad de iteraciones de warmup.
    warmup: usize,
    /// Mediana del tiempo de frontend en milisegundos.
    front_med_ms: f64,
    /// IQR del tiempo de frontend en milisegundos.
    front_iqr_ms: f64,
    /// Mediana del tiempo de compilación en milisegundos.
    comp_med_ms: f64,
    /// IQR del tiempo de compilación en milisegundos.
    comp_iqr_ms: f64,
    /// Mediana del tiempo de VM o ejecución en milisegundos.
    vm_med_ms: f64,
    /// IQR del tiempo de VM o ejecución en milisegundos.
    vm_iqr_ms: f64,
    /// Mediana del tiempo total en milisegundos.
    total_med_ms: f64,
    /// IQR del tiempo total en milisegundos.
    total_iqr_ms: f64,
    /// Cantidad de instrucciones generadas.
    instructions: usize,
    /// Cantidad de constantes generadas.
    constants: usize,
    /// Tamaño aproximado en bytes del bytecode.
    approx_bytes: usize,
    /// Estado o mensaje de error de la ejecución.
    status: String,
}

/// Punto de entrada del harness de benchmarking. Ejecuta pruebas según la configuración provista.
fn main() {
    let config = parse_args();

    println!("=== rlox benchmark runner ===");
    println!(
        "Configuración: warmup={}, reps={}, backend={:?}, opt={}",
        config.warmup, config.reps, config.backend, config.opt_config
    );
    println!("Archivos evaluados: {}", config.files.len());
    println!();

    println!(
        "{:<16} {:<7} {:<8} {:>9} {:>9} {:>10} {:>9} {:>6} {:>6} {:>8}",
        "benchmark",
        "backend",
        "opt",
        "front(ms)",
        "comp(ms)",
        "vm/run(ms)",
        "total(ms)",
        "ops",
        "const",
        "bytes"
    );
    println!("{}", "-".repeat(96));

    let mut csv_records: Vec<CsvRecord> = Vec::new();

    for path in &config.files {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let source = fs::read_to_string(path).expect("leer archivo .lox");

        let backends_to_run = match config.backend {
            TargetBackend::Vm => vec!["vm"],
            TargetBackend::Reg => vec!["reg"],
            TargetBackend::Tree => vec!["tree"],
            TargetBackend::All => vec!["vm", "reg", "tree"],
        };

        for b in backends_to_run {
            match b {
                "vm" => match bench_vm_program(
                    &source,
                    config.warmup,
                    config.reps,
                    config.opt_config.clone(),
                ) {
                    Ok((s_front, s_comp, s_vm, s_tot, stats, _out)) => {
                        println!(
                            "{:<16} {:<7} {:<8} {:>9.3} {:>9.3} {:>10.1} {:>9.1} {:>6} {:>6} {:>8}",
                            stem,
                            "vm",
                            format!("{:?}", config.opt_config.level),
                            s_front.median_ms,
                            s_comp.median_ms,
                            s_vm.median_ms,
                            s_tot.median_ms,
                            stats.instructions,
                            stats.constants,
                            stats.approx_bytes,
                        );
                        csv_records.push(CsvRecord {
                            benchmark: stem.clone(),
                            backend: "vm".to_string(),
                            opt_level: format!("{:?}", config.opt_config.level),
                            opt_passes: config.opt_config.enabled_summary(),
                            reps: config.reps,
                            warmup: config.warmup,
                            front_med_ms: s_front.median_ms,
                            front_iqr_ms: s_front.iqr_ms,
                            comp_med_ms: s_comp.median_ms,
                            comp_iqr_ms: s_comp.iqr_ms,
                            vm_med_ms: s_vm.median_ms,
                            vm_iqr_ms: s_vm.iqr_ms,
                            total_med_ms: s_tot.median_ms,
                            total_iqr_ms: s_tot.iqr_ms,
                            instructions: stats.instructions,
                            constants: stats.constants,
                            approx_bytes: stats.approx_bytes,
                            status: "ok".to_string(),
                        });
                    }
                    Err(e) => {
                        println!(
                            "{:<16} {:<7} ERROR: {}",
                            stem,
                            "vm",
                            e.lines().next().unwrap_or("")
                        );
                        csv_records.push(CsvRecord {
                            benchmark: stem.clone(),
                            backend: "vm".to_string(),
                            opt_level: format!("{:?}", config.opt_config.level),
                            opt_passes: config.opt_config.enabled_summary(),
                            reps: config.reps,
                            warmup: config.warmup,
                            front_med_ms: 0.0,
                            front_iqr_ms: 0.0,
                            comp_med_ms: 0.0,
                            comp_iqr_ms: 0.0,
                            vm_med_ms: 0.0,
                            vm_iqr_ms: 0.0,
                            total_med_ms: 0.0,
                            total_iqr_ms: 0.0,
                            instructions: 0,
                            constants: 0,
                            approx_bytes: 0,
                            status: format!("error: {e}"),
                        });
                    }
                },
                "reg" => match bench_reg_program(&source, config.warmup, config.reps) {
                    Ok((s_front, s_comp, s_vm, s_tot, stats, _out)) => {
                        println!(
                            "{:<16} {:<7} {:<8} {:>9.3} {:>9.3} {:>10.1} {:>9.1} {:>6} {:>6} {:>8}",
                            stem,
                            "reg",
                            "-",
                            s_front.median_ms,
                            s_comp.median_ms,
                            s_vm.median_ms,
                            s_tot.median_ms,
                            stats.instructions,
                            stats.constants,
                            stats.approx_bytes,
                        );
                        csv_records.push(CsvRecord {
                            benchmark: stem.clone(),
                            backend: "reg".to_string(),
                            opt_level: "n/a".to_string(),
                            opt_passes: "none".to_string(),
                            reps: config.reps,
                            warmup: config.warmup,
                            front_med_ms: s_front.median_ms,
                            front_iqr_ms: s_front.iqr_ms,
                            comp_med_ms: s_comp.median_ms,
                            comp_iqr_ms: s_comp.iqr_ms,
                            vm_med_ms: s_vm.median_ms,
                            vm_iqr_ms: s_vm.iqr_ms,
                            total_med_ms: s_tot.median_ms,
                            total_iqr_ms: s_tot.iqr_ms,
                            instructions: stats.instructions,
                            constants: stats.constants,
                            approx_bytes: stats.approx_bytes,
                            status: "ok".to_string(),
                        });
                    }
                    Err(e) => {
                        let short_msg = if e.contains("Classes are not supported yet") {
                            "unsupported (no classes)"
                        } else {
                            "unsupported"
                        };
                        println!("{:<16} {:<7} {:<8} {}", stem, "reg", "-", short_msg);
                        csv_records.push(CsvRecord {
                            benchmark: stem.clone(),
                            backend: "reg".to_string(),
                            opt_level: "n/a".to_string(),
                            opt_passes: "none".to_string(),
                            reps: config.reps,
                            warmup: config.warmup,
                            front_med_ms: 0.0,
                            front_iqr_ms: 0.0,
                            comp_med_ms: 0.0,
                            comp_iqr_ms: 0.0,
                            vm_med_ms: 0.0,
                            vm_iqr_ms: 0.0,
                            total_med_ms: 0.0,
                            total_iqr_ms: 0.0,
                            instructions: 0,
                            constants: 0,
                            approx_bytes: 0,
                            status: format!("unsupported: {e}"),
                        });
                    }
                },
                "tree" => match bench_tree_program(&source, config.warmup, config.reps) {
                    Ok((s_front, s_run, _out)) => {
                        println!(
                            "{:<16} {:<7} {:<8} {:>9.3} {:>9} {:>10.1} {:>9.1} {:>6} {:>6} {:>8}",
                            stem,
                            "tree",
                            "-",
                            s_front.median_ms,
                            "-",
                            s_run.median_ms,
                            s_front.median_ms + s_run.median_ms,
                            "-",
                            "-",
                            "-",
                        );
                        csv_records.push(CsvRecord {
                            benchmark: stem.clone(),
                            backend: "tree".to_string(),
                            opt_level: "n/a".to_string(),
                            opt_passes: "none".to_string(),
                            reps: config.reps,
                            warmup: config.warmup,
                            front_med_ms: s_front.median_ms,
                            front_iqr_ms: s_front.iqr_ms,
                            comp_med_ms: 0.0,
                            comp_iqr_ms: 0.0,
                            vm_med_ms: s_run.median_ms,
                            vm_iqr_ms: s_run.iqr_ms,
                            total_med_ms: s_front.median_ms + s_run.median_ms,
                            total_iqr_ms: s_run.iqr_ms,
                            instructions: 0,
                            constants: 0,
                            approx_bytes: 0,
                            status: "ok".to_string(),
                        });
                    }
                    Err(e) => {
                        println!("{:<16} {:<7} ERROR: {}", stem, "tree", e);
                    }
                },
                _ => unreachable!(),
            }
        }
    }

    if let Some(path) = &config.csv_path {
        let mut file = fs::File::create(path).expect("crear archivo CSV");
        writeln!(
            file,
            "benchmark,backend,opt_level,opt_passes,reps,warmup,front_med_ms,front_iqr_ms,comp_med_ms,comp_iqr_ms,vm_med_ms,vm_iqr_ms,total_med_ms,total_iqr_ms,instructions,constants,approx_bytes,status"
        ).unwrap();
        for r in csv_records {
            writeln!(
                file,
                "{},{},{},{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{},{},{},\"{}\"",
                r.benchmark,
                r.backend,
                r.opt_level,
                r.opt_passes,
                r.reps,
                r.warmup,
                r.front_med_ms,
                r.front_iqr_ms,
                r.comp_med_ms,
                r.comp_iqr_ms,
                r.vm_med_ms,
                r.vm_iqr_ms,
                r.total_med_ms,
                r.total_iqr_ms,
                r.instructions,
                r.constants,
                r.approx_bytes,
                r.status.replace('"', "\"\"")
            )
            .unwrap();
        }
        println!();
        println!("Resultados exportados a CSV: {}", path.display());
    }
}
