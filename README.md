# Diseño y decisiones

## Objetivo del proyecto

`rlox` implementa una primera versión de Lox en Rust siguiendo los lineamientos
generales del intérprete tree-walk, pero con una organización propia. El objetivo
de esta etapa es tener un lenguaje ejecutable, con errores legibles y una base que
pueda evolucionar hacia bytecode sin descartar el scanner ni el parser.

## Flujo de ejecución

El flujo principal está en `src/main.rs`:

```text
archivo o REPL
    -> Scanner
    -> Vec<Token>
    -> Parser
    -> Vec<Stmt> / AST
    -> Interpreter
    -> Value
```

El scanner conserva lexema, literal y línea. El parser recursivo descendente
construye expresiones y statements separados en `expr.rs` y `stmt.rs`. El
intérprete recorre esas estructuras mediante visitors y produce valores o
errores de runtime.

## Distribución de responsabilidades

| Archivo | Responsabilidad |
| --- | --- |
| `src/main.rs` | Entrada por archivo o REPL y coordinación de etapas |
| `src/scanner.rs` | Conversión de caracteres a tokens |
| `src/token.rs` | Tipos de token, literales y palabras reservadas |
| `src/parser.rs` | Parser, precedencia y recuperación ante errores |
| `src/expr.rs` | AST de expresiones y visitor de expresiones |
| `src/stmt.rs` | AST de statements y visitor de statements |
| `src/interpreter.rs` | Evaluación, control de flujo y operaciones |
| `src/environment.rs` | Variables y cadena de ambientes |
| `src/function.rs` | Funciones, argumentos, retornos y cierres |
| `src/value.rs` | Valores de runtime y truthiness |
| `real-tests/` | Programas de prueba de la cátedra |
| `examples/` | Ejemplos y casos exploratorios |

## Decisiones de diseño

### AST separado del lexer

El scanner sólo reconoce la sintaxis superficial y el parser decide la
estructura. Esto permite reportar errores de parsing antes de ejecutar y hace que
la futura compilación a bytecode pueda consumir el mismo AST o reemplazar esta
etapa de manera localizada.

### Visitors para expressions y statements

`Expr` y `Stmt` contienen datos, mientras que `Interpreter` contiene la
operación de evaluación. Esta separación evita mezclar semántica con la
representación del AST y deja abierta la posibilidad de agregar otro visitor. A cambio, cada nuevo nodo exige actualizar el trait y
sus implementaciones.

### Ambientes enlazados con `Rc<RefCell<Environment>>`

Cada bloque crea un ambiente que apunta a su enclosing environment. `Rc` permite
que una función conserve el ambiente donde fue declarada; `RefCell` permite
mutarlo durante la ejecución aunque el ownership sea compartido. Esta elección
modela naturalmente variables mutables y closures, pero introduce chequeos de
borrow en runtime y más indirecciones que una tabla local simple.

### Desazucarado de `for`

El parser transforma `for` en bloques y `while` antes de la interpretación. Así,
el intérprete sólo necesita implementar un mecanismo de loop y la semántica de
`for` queda expresada con construcciones ya existentes. El costo es que los
errores o dumps del AST muestran la forma transformada y no siempre la sintaxis
original.

### Valores dinámicos

`Value` representa números, strings, booleanos, `nil` y funciones. La validación
de operandos se hace al evaluar, que coincide con el modelo dinámico de Lox y
mantiene el parser sencillo.

### Elección del Lenguaje de Programación (Rust)

Se seleccionó Rust para la implementación de `rlox` por las siguientes ventajas técnicas sobre otros lenguajes:

* **Seguridad de Memoria Dinámica sin GC:** Rust garantiza *memory safety* mediante las reglas de ownership y borrado determinista en tiempo de compilación. Esto elimina la necesidad de contar con un *Garbage Collector* intermediario para administrar la memoria dinámica del árbol AST y las cadenas (`String`), maximizando la velocidad en ejecución.
* **Sistema de Tipos Expresivo y Exhaustivo:** El uso de enumerados (`enum`) permite representar de forma natural tanto los distintos nodos del AST (`Expr`/`Stmt`) como los tipos de datos en tiempo de ejecución (`Value`). Al combinar esto con coincidencia de patrones (*pattern matching*), el compilador exige verificar de forma exhaustiva todos los casos posibles antes de generar el ejecutable, eliminando errores por casos no contemplados.
* **Abstracciones sin Costo en Rendimiento (*Zero-Cost Abstractions*):** Permite diseñar el proyecto mediante patrones de diseño legibles (como el patrón *Visitor* e iteradores sobre los tokens del lexer) sin penalizaciones en velocidad. Durante la compilación en modo `--release`, el optimizador de Rust simplifica y traduce estas estructuras a código máquina plano.

## Ventajas y desventajas del tree-walk

### Ventajas

- implementación directa y fácil de depurar;
- cada construcción de Lox se puede seguir desde el AST hasta su evaluación;
- no requiere serializar bytecode ni diseñar un formato de instrucciones todavía;
- permite reutilizar ambientes y cierres para probar funciones complejas.

### Desventajas

- cada ejecución vuelve a recorrer el AST;
- el costo de la interpretación crece con la cantidad de nodos ejecutados;
- la representación actual no ofrece optimizaciones de bytecode ni cachés de instrucciones;
- los errores de runtime y el control por `Result` agregan trabajo a cada etapa.

## Benchmarks de Rendimiento (Entrega Parcial)

Se realizó una comparativa de rendimiento calculando la serie de Fibonacci recursiva (`fib(20)`) utilizando el ejecutable compilado en modo optimizado (`--release`).

### Resultados

| Entorno / Implementación | Tiempo Medio (`mean ± σ`) | Rendimiento Relativo |
| :--- | :--- | :--- |
| **C (`gcc`)** | **0.4 ms** ± 0.4 ms | **1.0x** (Base más rápida) |
| **Python 3** | **15.4 ms** ± 1.2 ms | ~36.1x más lento que C |
| **`rlox` (Tree-Walk)** | **24.8 ms** ± 1.3 ms | ~58.1x más lento que C |

### Metodología y Comando

Las mediciones se realizaron con la herramienta `hyperfine` aplicando 3 ejecuciones de calentamiento (`--warmup 3`):

```bash
# Compilación en modo release
cargo build --release
gcc ./examples/fib.c -o fib_c

# Ejecución del Benchmark
hyperfine --warmup 3 \
  './target/release/rlox ./examples/fib.lox' \
  'python3 -c "fib = lambda n: n if n <= 1 else fib(n-1) + fib(n-2); print(fib(20))"' \
  './fib_c'
```

### Análisis del Rendimiento

* **Código Nativo (`C`):** Al ser compilado directamente a instrucciones de máquina mediante `gcc`, el ejecutable `./fib_c` obtiene el tiempo de ejecución mínimo (~0.4 ms). 
* **Intérprete Tree-Walk (`rlox`) vs. Máquina Virtual (`Python 3`):**
  * **Sobrecarga de Recorrido Recursivo:** `rlox` evalúa el programa recorriendo dinámicamente el Árbol de Sintaxis Abstracta (AST) nodo por nodo mediante llamadas recursivas (`accept` / `visit_*`). Cada operación exige saltos constantes en memoria y despacho de métodos sobre el árbol por cada evaluación.
  * **Gestión de Entornos (*Scopes*):** Cada llamada recursiva en nuestro Tree-Walk asigna y resuelve variables mediante entornos enlazados dinámicamente (`Rc<RefCell<Environment>>`), lo que añade asignaciones en el *heap* y validación de préstamos en tiempo de ejecución (*borrow checking*) en cada paso de la recursión de Fibonacci.
  * **Bytecode de Python:** Python no recorre un AST durante la ejecución; primero traduce el código a una secuencia lineal de *bytecode* (`.pyc`) y la ejecuta sobre una Máquina Virtual en C altamente optimizada, evitando la navegación por punteros del árbol.
* **Proyección para la Entrega Final:** Para la Entrega Final, el paso a una arquitectura de *Bytecode Compiler* (emitiendo instrucciones planas para una Máquina Virtual basada en pila) eliminará el recorrido del AST y la sobrecarga de entornos dinámicos, reduciendo la brecha con Python.

### Bucles Anidados (`quad-loops.lox`)

Esta prueba evalúa el rendimiento en la asignación repetitiva de variables, evaluación de expresiones booleanas y control de flujo mediante bucles `while` ($100.000$ iteraciones en total).

| Implementación | Tiempo Medio (`mean ± σ`) | Rendimiento Relativo |
| :--- | :--- | :--- |
| **Python 3** | **21.1 ms** ± 6.4 ms | **1.0x** (Base) |
| **`rlox` (Tree-Walk)** | **24.6 ms** ± 3.4 ms | **1.16x** más lento que Python |

#### Comando Ejecutado

```bash
hyperfine --warmup 3 \
  './target/release/rlox ./examples/quad-loops.lox' \
  'python3 -c "i=0;
while i<1000:
    j=0
    while j<100: j+=1
    i+=1
print(i)"'
```

### Análisis del Resultado

A diferencia de la prueba de recursión (`fib.lox`), en la ejecución de bucles la diferencia entre `rlox` y Python 3 se reduce a solo un **16% de sobrecarga**. Esto demuestra que para iteraciones sin asignaciones de nuevos entornos de funciones (scopes de llamada), el intérprete *Tree-Walk* en Rust alcanza una velocidad de ejecución cercana a la máquina virtual de Python.

### Benchmark 3: Concatenación de Cadenas (`string-concat.lox`)

Esta prueba evalúa la eficiencia en la manipulación de texto y la asignación dinámica de memoria en el *heap* mediante un bucle de $20.000$ concatenaciones de cadenas.

| Implementación | Tiempo Medio (`mean ± σ`) | Rendimiento Relativo |
| :--- | :--- | :--- |
| **`rlox` (Tree-Walk)** | **3.7 ms** ± 0.6 ms | 
| **Python 3** | **17.0 ms** ± 3.1 ms | 

#### Comando Ejecutado

```bash
hyperfine --warmup 3 \
  './target/release/rlox ./examples/string-concat.lox' \
  'python3 -c "i=0; s=\"\";
while i<20000:
    s += \"a\"
    i += 1"'
```
### Análisis del Resultado

En operaciones de manipulación intensiva de texto, `rlox` superó a Python 3, siendo **4.55 veces más rápido** en completar las $20.000$ iteraciones de concatenación. Se infiere que las razones pueden ser:

* **Gestión de Memoria en el Heap:** En Rust, la concatenación de tipos `String` aprovecha búferes de capacidad contiguos en el *heap*. Cuando el búfer necesita reasignarse, Rust utiliza políticas de crecimiento geométrico optimizadas a nivel del sistema operativo.
* **Sobrecarga de Inmutabilidad en Python:** Las cadenas en Python (CPython) son estrictamente inmutables a nivel de lenguaje. Aunque CPython cuenta con optimizaciones internas en la instrucción `in-place` (`+=`), el runtime aún incurre en sobrecarga verificando recuentos de referencias (*reference counting*) para determinar si la cadena se puede modificar en el lugar o si debe clonar el objeto en cada iteración.
* **Ausencia de Garbage Collection Dinámico:** Mientras que Python mantiene un recolector de basura activo con control de referencias por cada asignación intermedia de texto, Rust destruye las instancias temporales del Heap de forma determinista mediante las reglas de ownership (propiedad) sin intervención de un Collector en tiempo de ejecución.

## Limitaciones de esta versión

- no hay compilador ni VM de bytecode;
- no hay clases, instancias ni herencia;
- no hay funcionalidad extra más allá del conjunto base implementado;
- no hay tests unitarios Rust;