# Suite de Benchmarks de `rlox`

Este directorio contiene los programas de prueba diseñados para evaluar el desempeño computacional de los backends de `rlox` (Tree-Walk, VM basada en pila y VM basada en registros).

---

## Estructura

```text
benchmarks/
├── programs/           # Programas .lox deterministas para medición
├── results/            # Archivos CSV inmutables y reportes de corridas
├── measure-e2e.ps1     # Script de medición end-to-end para Windows / PowerShell
├── ENV.md              # Especificaciones de hardware, entorno y comandos
└── README.md           # Esta guía de uso
```

---

## Programas de Benchmark (`programs/`)

| Programa | Enfoque principal | Carga de trabajo | Salida esperada | Soporte RegVm |
|---|---|---|---|---|
| `arith_locals.lox` | Aritmética pura y acceso a variables locales | 3.000.000 de iteraciones | `6750000750000` | Sí |
| `arith_globals.lox` | Acceso y asignación de variables globales | 3.000.000 de iteraciones | `6750000750000` | Sí |
| `const_expr.lox` | Expresiones de literales constantes en loop | 3.000.000 de iteraciones | `60000000` | Sí |
| `loops_nested.lox` | Sobrecarga de saltos y bucles anidados | 5.000 x 1.000 iteraciones | `5000000` | Sí |
| `calls.lox` | Llamadas a funciones de usuario con argumentos | 1.000.000 de llamadas | `499999500000` | Sí |
| `recursion.lox` | Recursión profunda (árbol de Fibonacci 27) | `fib(27)` (~392.835 llamadas) | `196418` | Sí |
| `closures.lox` | Upvalues abiertos y cerrados con invocaciones | 1.000.000 de llamadas | `1000001` | Sí |
| `methods.lox` | Creación de instancias, invocación de métodos y `this` | 1.000.000 de iteraciones | `500003500000` | No (sin clases) |
| `fields.lox` | Lectura y mutación intensiva de campos | 500.000 iteraciones (x4 campos) | `2000002` | No (sin clases) |
| `instantiation.lox` | Asignación y ciclo de vida de objetos e inicializadores | 300.000 instancias | `44999850000` | No (sin clases) |
| `inheritance.lox` | Cadenas de clases derivadas y despacho `super` | 200.000 instancias y llamadas | `20000300000` | No (sin clases) |
| `strings.lox` | Concatenación acumulativa y comparación de cadenas | 20.000 concats + 300.000 eq | `300000` | Sí |
| `print_heavy.lox` | I/O intensivo por consola (para medir impacto de buffers) | 50.000 impresiones | `49999` (último) | Sí |
| `mixed.lox` | Carga heterogénea: listas enlazadas, closures y bucles | 100 rondas de 2.000 nodos | `199905250` | No (sin clases) |

---

## Cómo ejecutar las mediciones

### 1. Medición interna con `rlox-bench`

El binario `rlox-bench` mide con precisión de microsegundos las fases de frontend, compilación y ejecución de la VM, aislando el overhead del sistema operativo:

```bash
cargo run --release --bin rlox-bench -- --warmup 3 --reps 10 --backend vm --csv benchmarks/results/baseline_vm.csv
```

Opciones:
- `--warmup N`: Cantidad de corridas de calentamiento previas (default: 3).
- `--reps N`: Cantidad de corridas cronometradas (default: 10).
- `--backend [vm|reg|tree|all]`: Backend a medir (default: `vm`).
- `--csv <ruta>`: Exporta tabla completa con medianas, IQR y tamaños a CSV.
- `<archivo1.lox> <archivo2.lox>`: Filtra qué programas ejecutar (si se omite, corre todos).

### 2. Medición End-to-End

```powershell
.\benchmarks\measure-e2e.ps1 -Backend "--vm" -Warmup 3 -Reps 10 -Csv "benchmarks/results/baseline_e2e_vm.csv"
```

