# Entorno Experimental y Metodología de Benchmarks

Este documento registra los parámetros de hardware, software, entorno operativo y procedimiento experimental empleados para obtener las mediciones baseline y las subsiguientes comparaciones de optimización en `rlox`.

---

## 1. Entorno de Hardware y Sistema Operativo

- **Procesador (CPU):** AMD Ryzen 5 3500 6-Core Processor (6 núcleos físicos, 6 hilos lógicos) @ 3.60 GHz base.
- **Memoria RAM:** 16 GB DDR4.
- **Sistema Operativo:** Microsoft Windows 11 Pro 64-bit (Build 10.0.26200).
- **Plan de Energía:** Equilibrado (`381b4222-f694-41f0-9685-ff5bb260df2e`).

---

## 2. Entorno de Compilación y Herramientas

- **Rust:** `rustc 1.98.1 (48a229cea 2026-09-01)`
- **Cargo:** `cargo 1.98.1 (797e8a9bc 2026-08-05)`
- **Git:** `git version 2.55.0.windows.3`
- **Perfil de Compilación:** `release` (`opt-level = 3`, `debug = false`).
- **Fecha de medición baseline:** 14 de Noviembre de 2026.

---

## 3. Identificación de la Baseline Inmutable

- **Tag de Código Productivo Baseline:** `opt-baseline`
- **Commit SHA:** `3cc44bc169673332f86c0f5adf8367675ece34fd`
- **Mensaje de commit:** `fix: correct NaN comparison semantics across backends`
- **Tag de Infraestructura de Benchmarks Baseline (Propuesto sobre E0):** `opt-bench-baseline`
- **Estado de tests:** 202 tests unitarios y de integración pasando (incluyendo paridad Tree-Walk y tests de NaN).
- **Identidad de código productivo:** Tanto `opt-baseline` como el commit resultante de E0 contienen exactamente el mismo código productivo en `src/` (las diferencias se limitan a `benchmarks/` y `src/bin/rlox-bench.rs`).

---

## 4. Metodología de Medición

Para aislar el ruido del sistema operativo y garantizar reproducibilidad:

1. **Compilación en modo Release:** Todos los benchmarks se ejecutan exclusivamente con binarios compilados con `cargo build --release`.
2. **Warmup (Calentamiento previo):** 3 ejecuciones previas descartadas para pre-calentar cachés de CPU y del sistema operativo.
3. **Repeticiones:** 10 ejecuciones cronometradas por cada programa y backend.
4. **Métricas Estadísticas:**
   - **Mediana:** Medida de tendencia central robusta ante valores atípicos (outliers).
   - **IQR (Rango Intercuartílico, $Q_3 - Q_1$):** Medida de dispersión robusta.
   - **Mínimo y Máximo:** Monitoreo de extremos.
5. **Captura de I/O en Memoria:** En el harness interno (`rlox-bench`), la salida estándar del intérprete se redirige a un buffer `Vec<u8>` en memoria para medir estrictamente el costo de computación y despacho de bytecode sin interferencia del buffer de terminal o I/O de Windows.
6. **Aislamiento de Fases:** El harness toma marcas de tiempo independientes para:
   - `front(ms)`: Escaneo léxico + parsing sintáctico + chequeo semántico estático.
   - `comp(ms)`: Generación de bytecode y resolución de variables en `compiler::compile`.
   - `vm(ms)`: Ejecución de la máquina virtual (`Vm::interpret`).
   - `total(ms)`: Tiempo de punta a punta del proceso del script en memoria.
7. **Métricas de Bytecode Recursivas:**
   - `ops`: Total de instrucciones `OpCode` emitidas acumulando el chunk del script y recursivamente todos los prototipos de funciones (`FnProto`) anidadas, métodos y closures declarados, garantizando no contar dos veces el mismo prototipo mediante deduplicación por puntero.
   - `const`: Cantidad total acumulada de entradas en los pools de constantes.
   - `bytes`: Tamaño en memoria aproximado de las estructuras de código, constantes y líneas de todos los chunks.

---

## 5. Comandos de Reproducción

### A. Harness interno (medición por fases)
```powershell
# Compilar harness
cargo build --release --bin rlox-bench

# Ejecutar baseline de la VM de pila sobre todos los benchmarks y exportar a CSV:
.\target\release\rlox-bench.exe --warmup 3 --reps 10 --backend vm --csv benchmarks/results/baseline_vm.csv

# Ejecutar sobre la VM de registros (los programas con clases saldrán marcados como unsupported):
.\target\release\rlox-bench.exe --warmup 3 --reps 10 --backend reg --csv benchmarks/results/baseline_reg.csv

# Comparar todos los backends:
.\target\release\rlox-bench.exe --warmup 3 --reps 10 --backend all
```

### B. Medición End-to-End (tiempo de vida del proceso)
```powershell
# Con el script PowerShell provisto:
powershell -ExecutionPolicy Bypass -File benchmarks\measure-e2e.ps1 -Backend "--vm" -Warmup 3 -Reps 10 -Csv "benchmarks/results/baseline_e2e_vm.csv"

# Si Hyperfine estuviera instalado en el sistema:
# cargo install hyperfine  (o scoop/choco install hyperfine)
hyperfine --warmup 3 -r 10 --export-csv benchmarks/results/hyperfine_baseline.csv '.\target\release\rlox.exe --vm benchmarks/programs/arith_locals.lox'
```

---

## 6. Procedimiento para Comparar con Worktrees Aislados

Dado que `opt-baseline` apunta al commit de corrección de código sin los binarios de benchmarking, existen dos alternativas perfectamente reproducibles para contrastar futuras ramas optimizadas:

### Alternativa 1: Medición End-to-End del binario compilado en `opt-baseline`
En `opt-baseline` sí existe el binario principal `src/main.rs` (`rlox.exe`):
```powershell
# 1. Crear worktree apuntando a la baseline de código:
git worktree add ../rlox-baseline opt-baseline

# 2. Compilar el ejecutable original en release:
cd ../rlox-baseline
cargo build --release --bin rlox

# 3. Volver al repositorio principal y medir el ejecutable de la baseline con el script E2E:
cd ../rlox-1
powershell -ExecutionPolicy Bypass -File benchmarks\measure-e2e.ps1 -Backend "--vm" -Warmup 3 -Reps 10 -Csv "benchmarks/results/check_baseline_e2e.csv"
```

### Alternativa 2: Uso del tag de infraestructura `opt-bench-baseline` (Recomendada)
Para usar el runner interno `rlox-bench` en un worktree aislado de referencia, se utiliza el tag `opt-bench-baseline` (que contiene el mismo código productivo que `opt-baseline` más el harness E0):
```powershell
# 1. Crear worktree aislado de la baseline de benchmarking:
git worktree add ../rlox-baseline opt-bench-baseline

# 2. Compilar y ejecutar la baseline:
cd ../rlox-baseline
cargo build --release --bin rlox-bench
.\target\release\rlox-bench.exe --backend vm --csv ../rlox-1/benchmarks/results/check_baseline.csv

# 3. Volver a la rama de desarrollo optimizada:
cd ../rlox-1
```

