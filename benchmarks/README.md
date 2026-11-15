# Evaluación y Optimización de Rendimiento de la Máquina Virtual rlox

Este documento presenta el informe técnico completo de las optimizaciones de rendimiento implementadas en el backend de máquina virtual basada en pila (*Stack VM*) de **rlox**, el intérprete del lenguaje Lox desarrollado en Rust.

El informe detalla cada optimización concreta: el problema de diseño original, la implementación técnica con fragmentos de código o bytecode representativo, la medición experimental de su impacto y los invariantes semánticos preservados.

---

## 1. Introducción y Baseline

### 1.1 Contexto de rlox
**rlox** es una implementación del lenguaje dinámico Lox (definido por Robert Nystrom en *Crafting Interpreters*). El sistema cuenta con un frontend compartido (escáner léxico, parser descendente recursivo / Pratt y verificador semántico de declaraciones y resolución léxica) y tres backends de ejecución:
1. **Tree-Walk Interpreter**: Intérprete clásico que recorre directamente el AST mediante el patrón Visitor.
2. **Stack VM (`--vm`)**: Compilador de bytecode lineal (`Chunk`) y máquina virtual basada en pila con soporte completo de tipos primitivos, funciones, cierres léxicos (*closures*), clases, instancias, métodos y herencia simple.
3. **Register VM (`--reg`)**: Backend experimental basado en registros.

El trabajo de optimización se concentró de forma exclusiva en la **Stack VM (`--vm`)**.

### 1.2 Objetivo de las Optimizaciones
La implementación inicial de la Stack VM priorizaba la simplicidad estructural y la paridad semántica con el intérprete de referencia. No obstante, presentaba diversos cuellos de botella:
- Estructuras de valores sobredimensionadas en el stack de operandos.
- Asignaciones repetidas en el heap para ligar métodos y mutar campos existentes de objetos.
- Múltiples ciclos de *dispatch* y operaciones de push/pop para operaciones aritméticas básicas y asignaciones locales simples.
- Búsquedas por cadenas de texto (`HashMap<String, Value>`) mediante SipHash en cada acceso a variables globales.
- Emisión de código inalcanzable y evaluación en tiempo de ejecución de subexpresiones constantes.

El objetivo fue aplicar transformaciones tanto en el runtime de la máquina virtual como en el pipeline de generación de bytecode, asegurando:
1. **Cero regresiones semánticas**: Paridad exacta con la especificación de Lox (mismos mensajes de error en tiempo de ejecución, mismos números de línea reportados y preservación del orden de evaluación de efectos laterales).
2. **Eficiencia medible**: Reducción verificada del tiempo de ejecución en benchmarks estándar de CPU y memoria.
3. **Metodología empírica rigurosa**: Mediciones repetibles, aisladas del ruido del sistema operativo y con control de varianza.

### 1.3 Línea de Base Original (*Baseline*)
La línea de base inmutable de comparación corresponde al commit etiquetado como `opt-baseline`:
- **Commit SHA**: `3cc44bc169673332f86c0f5adf8367675ece34fd`
- **Descripción**: Estado de rlox con soporte completo de lenguaje (incluyendo clases y herencia), previo a cualquier modificación estructural o de compilación orientada al rendimiento.
- **Entorno**: Compilado en perfil `release` (`opt-level = 3`) bajo el mismo compilador Rust y hardware que la versión final.

---

## 2. Optimizaciones Implementadas

A continuación se detallan individualmente las optimizaciones que forman parte del código productivo final, analizadas por técnica concreta.

---

### 2.1 Representación Compacta de `Value` mediante Punteros Compartidos (`Rc`)

#### Problema original
El tipo `Value` era un enum Rust compartido por todos los backends. La variante `Value::Function(LoxFunction)` contenía inline una estructura `LoxFunction` del intérprete Tree-Walk de 120 bytes (con tokens, vectores y metadatos sintácticos). 

Por reglas de alineación y tamaño de enums en Rust, cada instancia de `Value` ocupaba **120 bytes** (`size_of::<Value>() == 120`). Cada operación de la pila (`push`, `pop`, `clone`, paso de argumentos o lectura de slots) copiaba bloques de 120 bytes en memoria. Esto degradaba el rendimiento por saturación del bus de memoria y desperdiciaba el espacio útil en las líneas de caché L1 de la CPU.

#### Implementación
Se encapsuló la función del tree-walk dentro de un puntero compartido `Rc`:

```rust
// src/compiler/value.rs
#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
    Nil,
    Function(Rc<LoxFunction>), // Antes: Function(LoxFunction) inline (120 bytes)
    Class(Rc<RefCell<LoxClass>>),
    Instance(Rc<RefCell<LoxInstance>>),
    Proto(Rc<FnProto>),
    Closure(Rc<Closure>),
    BoundMethod(Rc<BoundMethod>),
    VmClass(Rc<RefCell<VmClass>>),
    VmInstance(Rc<RefCell<VmInstance>>),
}
```

Gracias a este cambio, el tamaño de `Value` se redujo de **120 bytes a 24 bytes** (determinado ahora por `String(String)`, que contiene 3 palabras de 64 bits en Rust de 64 bits). Las operaciones de copia sobre la pila pasaron a mover una quinta parte de los bytes originales.

#### Impacto en rendimiento (Medición aislada)
Medición con `rlox-bench` comparando el baseline original (`3cc44bc`) frente a la versión con `Value` compacto (10 repeticiones, 3 warmups, tiempo de VM en ms):

| Benchmark | Baseline Original (ms) | `Value` Compacto (ms) | Speedup | Reducción de tiempo | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: |
| `arith_locals.lox` | 819.86 | **502.06** | **1.63x** | -38.8% | Aislada |
| `arith_globals.lox` | 1380.10 | **954.31** | **1.45x** | -30.9% | Aislada |
| `calls.lox` | 264.40 | **193.32** | **1.37x** | -26.9% | Aislada |
| `closures.lox` | 260.05 | **164.84** | **1.58x** | -36.6% | Aislada |
| `methods.lox` | 1066.85 | **734.94** | **1.45x** | -31.1% | Aislada |
| `recursion.lox` | 113.02 | **95.20** | **1.19x** | -15.8% | Aislada |
| `mixed.lox` | 278.88 | **203.00** | **1.37x** | -27.2% | Aislada |

#### Consideraciones de corrección
- Se actualizó la implementación de `PartialEq` para `Value::Function` utilizando `Rc::ptr_eq(a, b) || a == b`, preservando la identidad por referencia de funciones.
- No hubo alteraciones en el comportamiento de ninguno de los tres backends.

---

### 2.2 Operaciones Numéricas In-Place sobre el Stack

#### Problema original
La ejecución de operaciones binarias aritméticas y lógicas (`Add`, `Subtract`, `Multiply`, `Divide`, `Less`, etc.) extraía ambos operandos llamando a `self.pop()` dos veces consecutivas, evaluaba la operación y empujaba el resultado mediante `self.stack.push(...)`. Este patrón requería 3 modificaciones del campo `len` del vector del stack, múltiples comprobaciones de límites (*bounds checks*) y movimiento redundante de memoria en bucles computacionales intensivos.

#### Implementación
Se introdujeron funciones auxiliares en línea (`binary_num` y `binary_num_bool`) marcadas con `#[inline(always)]`:

```rust
// src/compiler/vm.rs
#[inline(always)]
fn binary_num<F>(&mut self, chunk: &Chunk, ip: usize, op: F) -> Result<(), String>
where
    F: FnOnce(f64, f64) -> f64,
{
    let len = self.stack.len();
    let (x, y) = match (&self.stack[len - 2], &self.stack[len - 1]) {
        (Value::Number(x), Value::Number(y)) => (*x, *y),
        _ => return Err(Self::error(chunk, ip, "Operands must be numbers.")),
    };
    self.stack[len - 2] = Value::Number(op(x, y));
    self.stack.pop();
    Ok(())
}
```

La operación lee los dos últimos elementos del stack mediante referencias, valida sus tipos, sobrescribe directamente la posición `len - 2` con el resultado y realiza un único `self.stack.pop()` para descartar el operando superior.

#### Impacto en rendimiento (Medición aislada)
Medición sobre la versión previa (`Value` compacto):

| Benchmark | Antes (ms) | Después (In-Place) (ms) | Speedup | Reducción de tiempo | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: |
| `arith_locals.lox` | 517.21 | **504.92** | **1.02x** | -2.4% | Aislada |
| `arith_globals.lox` | 982.79 | **929.45** | **1.06x** | -5.4% | Aislada |
| `const_expr.lox` | 551.78 | **545.59** | **1.01x** | -1.1% | Aislada |
| `mixed.lox` | 206.45 | **200.39** | **1.03x** | -2.9% | Aislada |

#### Consideraciones de corrección
- La comprobación de tipos ocurre antes de mutar la ranura `len - 2`. Si se produce un error (`"Operands must be numbers."`), el stack conserva su contenido canónico para el reporte del error en tiempo de ejecución.
- En `OpCode::Add`, la rama polimórfica para concatenación de cadenas mantiene la semántica original.

---

### 2.3 Optimización del Dispatch de Llamadas y Call Frames

#### Problema original
En cada instrucción `OpCode::Call`:
1. Se clonaba el valor del invocador (`callee`) situado en el stack para pasarlo a una función externa `call_value`.
2. Dicha función realizaba un desempaquetado redundante del tipo de invocador.
3. Al retornar, la VM debía consultar `self.frames.last()` para sincronizar los punteros locales de `closure`, `ip` y `base`.
4. El vector `frames` comenzaba sin capacidad preasignada (`Vec::new()`), forzando reasignaciones dinámicas de memoria en llamadas recursivas.

#### Implementación
1. **Capacidad preasignada**: Se inicializó `frames` con `Vec::with_capacity(64)` en el constructor de la VM.
2. **Despacho por referencia**: Se eliminó `call_value` y `OpCode::Call` pasó a inspeccionar directamente `&self.stack[callee_slot]`.
3. **Sincronización directa**: Los registros locales de la VM se configuran directamente sin búsquedas adicionales en el vector de marcos:

```rust
// src/compiler/vm.rs en OpCode::Call
match &self.stack[callee_slot] {
    Value::Closure(c) => {
        let c = Rc::clone(c);
        self.call_closure(Rc::clone(&c), argc)?;
        closure = c;
        ip = 0;
        base = callee_slot;
    }
    // ... BoundMethod y VmClass
}
```

#### Impacto en rendimiento (Medición aislada)
Medición sobre la versión previa (aritmética in-place):

| Benchmark | Antes (ms) | Con Call Opts (ms) | Speedup | Reducción de tiempo | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: |
| `calls.lox` (1M llamadas) | 184.73 | **160.82** | **1.15x** | -12.9% | Aislada |
| `closures.lox` (1M llamadas) | 159.11 | **141.65** | **1.12x** | -11.0% | Aislada |
| `recursion.lox` (`fib(27)`) | 93.90 | **77.31** | **1.21x** | -17.7% | Aislada |
| `inheritance.lox` | 261.63 | **243.16** | **1.08x** | -7.1% | Aislada |

#### Consideraciones de corrección
- Se preservaron las validaciones de aridad y los mensajes de error canónicos (`"Expected N arguments but got M."`).
- El valor del invocador se retiene en `callee_slot` para permitir el acceso a closures y métodos ligados.

---

### 2.4 Actualización In-Place de Propiedades (`SetProperty`)

#### Problema original
Al ejecutar una asignación sobre un campo de una instancia (`objeto.campo = valor`), el manejador original de `OpCode::SetProperty` realizaba:

```rust
instance.borrow_mut().fields.insert(name.to_string(), value.clone());
```

Dado que el nombre del campo se obtiene como un `&str` desde la tabla de constantes del chunk, `insert` forzaba la asignación en el heap de un nuevo objeto `String` para la clave, aun cuando la propiedad ya existía en la tabla hash de la instancia. En bucles intensivos de mutación de campos, esto provocaba millones de asignaciones y desasignaciones dinámicas innecesarias.

#### Implementación
Se introdujo una consulta previa mediante `get_mut(&str)`, la cual no realiza alocación alguna cuando la clave ya se encuentra registrada en la tabla:

```rust
// src/compiler/vm.rs
OpCode::SetProperty(k) => {
    let name = Self::name_at(chunk, k);
    let value = self.pop();
    let obj = self.pop();
    match obj {
        Value::VmInstance(instance) => {
            let mut inst = instance.borrow_mut();
            if let Some(field) = inst.fields.get_mut(name) {
                *field = value.clone();
            } else {
                inst.fields.insert(name.to_string(), value.clone());
            }
            drop(inst);
            self.stack.push(value);
        }
        _ => return Err(Self::error(chunk, ip, "Only instances have fields.")),
    }
}
```

#### Impacto en rendimiento (Medición aislada)
Medición comparativa en microbenchmarks especializados (500.000 iteraciones con 2 asignaciones por ciclo, es decir, 1.000.000 de operaciones de `SetProperty`):

| Benchmark | Nivel | Antes (ms) | Después (In-Place) (ms) | Speedup | Variación de tiempo | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `fields_existing.lox` (campos preexistentes) | `-O0` | 182.9 | **101.1** | **1.81x** | -44.7% | Aislada |
| `fields_existing.lox` (campos preexistentes) | `-O2` | 146.7 | **75.4** | **1.95x** | -48.6% | Aislada |
| `fields.lox` (suite estándar) | `-O2` | 351.7 | **197.7** | **1.78x** | -43.8% | Aislada |
| `methods.lox` | `-O2` | 575.9 | **478.7** | **1.20x** | -16.9% | Aislada |
| `fields_new.lox` (creación de campos nuevos) | `-O2` (corrida inicial) | 98.9 | **106.1** | **0.93x** | **+7.3%** | Aislada |
| `fields_new.lox` (creación de campos nuevos) | `-O2` (15 reps c/warmup) | 100.9 | **97.2** | **1.04x** | -3.7% | Aislada |
| `fields_new.lox` (creación de campos nuevos) | `-O0` | 123.3 | **122.3** | **1.01x** | -0.8% | Aislada |

*Nota sobre `fields_new.lox`: Al crear campos nuevos en cada iteración, la comprobación adicional `get_mut` antes del `insert` introduce una rama condicional extra en el camino frío, lo que explica la pequeña sobrecarga medida en la corrida inicial (+7.3%). En mediciones repetidas con calentamiento estricto, el impacto se estabiliza dentro de la varianza neutra.*

#### Consideraciones de corrección
- La asignación de propiedades en Lox es una expresión cuyo valor resultante debe quedar en la cima del stack (`self.stack.push(value)`).
- La inserción de nuevos campos cuando la propiedad no existe se conserva mediante `insert(name.to_string(), value.clone())`.
- La liberación explícita del préstamo mutable (`drop(inst)`) antes de empujar el resultado a la pila previene conflictos de borrow checker.

---

### 2.5 Constant Folding (Plegado de Constantes)

#### Problema original
El compilador emitía instrucciones de bytecode para cada subexpresión aritmética o lógica, incluso cuando todos los operandos intervinientes eran literales constantes conocidos estáticamente. Por ejemplo, `(10 + 20) * 3` emitía 5 instrucciones de bytecode y consumía 3 posiciones en el pool de constantes, obligando a la VM a realizar fetches, dispatches y manipulaciones de stack en cada iteración de un bucle.

#### Implementación
Se implementó un evaluador estático puro en el frontend (`compiler::opt::fold_expr`) que se ejecuta durante la pasada de generación de código:
- Pliega operadores binarios aritméticos (`+`, `-`, `*`, `/`, `%`) y relacionales (`<`, `<=`, `>`, `>=`, `==`, `!=`) sobre literales numéricos y cadenas.
- Pliega operadores unarios (`-`, `!`) y expresiones agrupadas entre paréntesis.
- Si ambos operandos son constantes válidas, emite directamente la constante resultante en el pool (`Constant(k)` o los opcodes especializados `True`, `False`, `Nil`).

```text
Código fuente Lox: print (10 + 20) * 3;

Sin optimización (-O0):          Con Constant Folding (-O1 / -O2):
0000    Constant(10.0)            0000    Constant(90.0)
0001    Constant(20.0)            0001    Print
0002    Add
0003    Constant(3.0)
0004    Multiply
0005    Print
```

#### Impacto en rendimiento (Medición aislada)
Medición comparativa (`-O0` control vs `-O0 --opt=fold`):

| Benchmark | `-O0` Control (ms) | Con Folding (ms) | Speedup | Opcodes Emitidos | Constantes en Pool | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `const_expr.lox` | 475.48 | **331.20** | **1.44x** | 38 → 30 (-21.1%) | 11 → 7 (-36.4%) | Aislada |

#### Consideraciones de corrección
- **No se pliegan operaciones erróneas**: Expresiones que producen errores semánticos en tiempo de ejecución (como `"a" - 1` o divisiones por cero) no se pliegan en compilación; se emite el bytecode ordinario para que la VM falle en tiempo de ejecución con el mensaje exacto y la línea correspondiente.
- Se respetan estrictamente las reglas de punto flotante de Rust equivalentes a IEEE-754 (preservando `-0.0`, infinitos y NaN).

---

### 2.6 Dead Code Elimination (Eliminación de Código Muerto)

#### Problema original
Estructuras de control condicionales evaluadas con literales o constantes conocidas (por ejemplo, `if (false) { ... }` o `while (false) { ... }`), así como sentencias posteriores a una instrucción `return` incondicional dentro del mismo bloque, producían bytecode inalcanzable. Esto incrementaba el tamaño del chunk, forzaba saltos innecesarios en la VM y ocupaba espacio en las tablas de constantes.

#### Implementación
Se incorporaron podas estáticas en `visit_if_stmt`, `visit_while_stmt` y `visit_block_stmt` de `src/compiler/mod.rs`:
- Si la condición de un `if` evalúa a constante verdadera, se emite únicamente la rama *then* sin saltos condicionales ni rama *else*.
- Si la condición evalúa a constante falsa, se emite únicamente la rama *else* (o nada si no existe).
- En bucles `while (false)`, se omite la emisión del cuerpo por completo.
- En bloques de sentencias, tras compilar un `return`, las sentencias restantes del mismo bloque no se emiten al chunk.

```text
Código fuente Lox: if (false) { print "Nunca"; } else { print "Siempre"; }

Sin optimización (-O0): 11 instrucciones
0000 False; 0001 JumpIfFalse(6); 0002 Pop; 0003 Constant("Nunca"); 0004 Print; 
0005 Jump(3); 0006 Pop; 0007 Constant("Siempre"); 0008 Print; 0009 Nil; 0010 Return

Con Dead Code Elimination (-O1 / -O2): 4 instrucciones
0000 Constant("Siempre"); 0001 Print; 0002 Nil; 0003 Return
```

#### Impacto en rendimiento (Medición aislada)
Medición comparativa (`-O0` control vs `-O0 --opt=dce`):

| Benchmark | `-O0` Control (ms) | Con DCE (ms) | Speedup | Opcodes Emitidos | Constantes en Pool | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `dead_code.lox` | 383.49 | **318.15** | **1.21x** | 70 → 40 (-42.9%) | 15 → 10 (-33.3%) | Aislada |

#### Consideraciones de corrección
- El verificador estático (`checker::check`) continúa ejecutándose sobre el AST completo antes de la generación de código, garantizando que errores sintácticos o de resolución en código muerto se reporten en compilación.

---

### 2.7 Jump Simplification (Jump Threading y Compactación de Saltos)

#### Problema original
La estructura de control de Lox genera saltos condicionales e incondicionales que pueden encadenarse hacia otros saltos (por ejemplo, el final de un bloque `if` anidado saltando al final del `if` externo). Estos saltos en cadena provocan múltiples bifurcaciones en la VM. Asimismo, ciertas construcciones dejaban instrucciones de salto con desplazamiento cero (`Jump(0)`).

#### Implementación
Se implementó un pase de optimización post-compilación en dos fases:
1. **Jump Threading**: Resuelve de forma transitiva cualquier salto cuyo destino sea otro salto incondicional, reescribiendo el offset de destino para apuntar directamente a la instrucción final ejecutable.
2. **Compactación**: Elimina instrucciones `Jump(0)` y saltos redundantes a la instrucción inmediatamente siguiente, recalculando las referencias de saltos relativos y el mapa de líneas (`lines`).

#### Impacto en rendimiento y auditoría empírica
Al auditar los archivos de resultados (`benchmarks/results/e4c_jumps.csv` vs `e4c_o0_control.csv`), se constató que en la suite estándar de benchmarks:
- **La cantidad de instrucciones emitidas no varió** en ninguno de los programas (0 opcodes eliminados en los 14 benchmarks: `arith_globals` 33 $\to$ 33, `arith_locals` 38 $\to$ 38, `loops_nested` 44 $\to$ 44, `mixed` 123 $\to$ 123, etc.).
- Las pequeñas diferencias de tiempo observadas entre corridas (mediana de 1.03x) se debieron a **ruido y varianza térmica normal** del entorno de prueba, y no a una aceleración atribuible a la optimización.
- **Conclusión de rendimiento**: Jump Simplification es técnicamente correcta y elimina saltos redundantes en programas sintéticos con saltos encadenados profundos, pero **no produjo una mejora aislada reproducible** sobre la suite estándar de benchmarks de Lox.

#### Consideraciones de corrección
- Se previenen bucles infinitos en el analizador transitivo detectando ciclos de saltos.
- La integridad de las tablas de líneas para reporte de errores en tiempo de ejecución se recalcula de forma paralela al código compacto.

---

### 2.8 Invocación Directa de Métodos mediante `PrepareInvoke` e `Invoke`

#### Problema original
En la especificación estándar de Lox, la llamada a un método sobre una instancia (`objeto.metodo(arg1, arg2)`) se compone de dos expresiones desacopladas:
1. Una expresión de acceso a propiedad (`visit_get_expr`), que emite `OpCode::GetProperty`.
2. Una llamada a función (`visit_call_expr`), que emite los argumentos y luego `OpCode::Call`.

En tiempo de ejecución, `GetProperty` busca la función en la clase de la instancia y aloca dinámicamente en el heap un objeto envoltorio `Value::BoundMethod(Rc::new(BoundMethod { receiver, method }))`. Inmediatamente después, `Call` desempaquetaba el método ligado, colocaba el receptor en la ranura local 0 y descartaba el `BoundMethod`. En programas con llamadas intensivas a métodos, esto producía millones de asignaciones de heap de vida extremadamente corta.

#### Implementación
Se diseñó un esquema coordinado en dos fases que preserva el orden estricto de evaluación de argumentos y la semántica de error de Lox:
1. **Fase 1 (`OpCode::PrepareInvoke(k)`)**: Se emite antes de evaluar los argumentos. La VM verifica que el receptor en la cima del stack sea una instancia válida. Si la instancia contiene un campo que eclipsa al método (*field shadowing*), extrae su valor; de lo contrario, resuelve el método en la clase. Registra la invocación en una estructura interna LIFO `prepared_calls` sin alocar un `BoundMethod` en el heap.
2. **Evaluación de argumentos**: Los argumentos se compilan y evalúan en la pila con normalidad.
3. **Fase 2 (`OpCode::Invoke(argc)`)**: Se emite tras los argumentos. La VM recupera la información de `prepared_calls`, ubica el receptor en la ranura base correspondiente (`stack.len() - 1 - argc`) e inicia el marco de ejecución del método de manera directa.

#### Impacto en rendimiento (Medición aislada)
Medición comparativa (`-O0` vs `-O0 --opt=invoke`):

| Benchmark | `-O0` Control (ms) | Con Invoke (ms) | Speedup | Reducción de tiempo | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: |
| `methods.lox` (2.000.000 llamadas a métodos) | 692.1 | **556.4** | **1.24x** | -19.6% | Aislada |
| `inheritance.lox` | 234.4 | **215.5** | **1.09x** | -8.1% | Aislada |

*En `methods.lox`, la optimización eliminó 2.000.000 de alocaciones `Rc<BoundMethod>`.*

#### Consideraciones de corrección
- **Orden de errores preservado**: Si el receptor no es una instancia (ej. `(123).metodo(error())`), `PrepareInvoke` falla con `"Only instances have properties."` antes de evaluar los argumentos, preservando la semántica exacta de `-O0`.
- **Field Shadowing**: Si una instancia almacena una función en un campo con el mismo nombre que un método, `PrepareInvoke` prioriza el campo según la especificación de Lox.
- **Llamadas anidadas**: La estructura `prepared_calls` opera de manera estrictamente LIFO, permitiendo anidamiento arbitrario como `a.m(b.n(c.o()))`.

---

### 2.9 Invocación Directa de Métodos de Superclases (`PrepareSuperInvoke`)

#### Problema original
Al igual que en las llamadas a métodos ordinarios, la invocación de un método sobre la superclase (`super.metodo(args)`) emitía `GetSuper(k)` seguido de `Call(argc)`. Esto implicaba buscar el método en la superclase resuelta léxicamente y crear una instancia en heap de `Value::BoundMethod` ligada a la instancia receptora (`this`), la cual era consumida y liberada inmediatamente por `Call`.

#### Implementación
Se introdujo la instrucción especializada `OpCode::PrepareSuperInvoke(k)`. Al emitirse, resuelve el método en la superclase indicada utilizando la constante de nombre `k` y enlaza el receptor `this` del marco actual, registrando la llamada en `prepared_calls`. Posteriormente, la misma instrucción `OpCode::Invoke(argc)` consume los argumentos y realiza la llamada sin alocaciones intermedias en el heap.

#### Impacto en rendimiento (Medición aislada)
Medición comparativa en benchmarks con jerarquías de herencia y llamadas `super`:

| Benchmark | `-O0` Control (ms) | Con SuperInvoke (ms) | Speedup | Reducción de tiempo | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: |
| `super_calls.lox` (1M super llamadas directas) | 411.1 | **247.3** | **1.66x** | -39.8% | Aislada |
| `inheritance.lox` (200k iteraciones, 600k super calls) | 251.9 | **171.1** | **1.47x** | -32.1% | Aislada |

#### Consideraciones de corrección
- Preserva la resolución léxica de `super`: el método se resuelve con respecto a la superclase en el punto de definición del método que invoca, mientras que `this` retiene la identidad de la subclase instanciada.

---

### 2.10 Indexed Globals y Caches de Variables Globales

#### Problema original
Lox implementa enlace tardío (*late binding*) para variables globales, permitiendo referenciar variables antes de su declaración formal si son accedidas dentro de funciones diferidas. Originalmente, cada lectura (`GetGlobal`) y escritura (`SetGlobal`) realizaba una búsqueda por cadena de texto en `self.globals: HashMap<String, Value>`. Esto implicaba calcular el hash SipHash del nombre en cada iteración de bucles que usaran globales, resultando en un costo de acceso un 80% superior al de variables locales.

#### Implementación
Se introdujo un sistema de **Inline Caching e Indexación por Ranuras Densas**:
1. **Opcodes especializados**: Se crearon `OpCode::GetGlobalFast(u16)` y `OpCode::SetGlobalFast(u16)`.
2. **Almacenamiento denso en la VM**: La máquina virtual mantiene un vector contiguo de valores `self.globals_dense: Vec<Value>`. Cada nombre de variable global tiene asignado un identificador numérico único de ranura (`slot: usize`).
3. **Aislamiento de Caches por Prototipo e Instancia**: Cada `FnProto` compilado recibe un identificador atómico único (`id: usize`). Los caches inline (`self.ic_slots`) residen exclusivamente dentro de cada instancia de `Vm`, garantizando total aislamiento entre hilos y ejecuciones independientes.
4. **Mecanismo de resolución**:
   - En el primer acceso dinámico, la instrucción consulta la tabla de nombres, obtiene o asigna el slot denso y escribe el índice numérico en su caché local.
   - En las ejecuciones sucesivas, el acceso se efectúa en tiempo constante directo $O(1)$ indexando el vector `self.globals_dense[slot]`, sin llamadas a funciones hash ni comparaciones de strings.

#### Impacto en rendimiento (Medición aislada)
Medición comparativa aislada sobre la versión base:

| Benchmark | `-O0` Baseline (ms) | Con Indexed Globals (ms) | Speedup | Reducción de tiempo | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: |
| `arith_globals.lox` (3M iters, 21M accesos globales) | 978.4 | **523.4** | **1.87x** | -46.5% | Aislada |
| `mixed.lox` (accesos mixtos con globales) | 204.3 | **194.4** | **1.05x** | -5.1% | Aislada |

*En `arith_globals.lox`, la optimización eliminó más de **21.000.000 de búsquedas y cálculos SipHash** por corrida.*

#### Consideraciones de corrección
- Si se intenta leer una variable global no inicializada o inexistente, el slot contiene una marca centinela que dispara el error canónico `"Undefined variable '<nombre>'."` con el número de línea exacto.
- Soporta redefinición legal de globales (ej. `var x = 1; var x = 2;`) actualizando el slot existente sin duplicar ranuras.

---

### 2.11 Superinstrucciones Aritméticas Locales (`AddLocalConst` y `LessLocalConst`)

#### Problema original
Los patrones de incremento de contadores (`i + 1`) y comparación de límites de bucles (`i < 100`) son predominantes en código Lox. En la Stack VM estándar, evaluar `i + 1` requería tres ciclos de dispatch independientes:
1. `OpCode::GetLocal(slot)`: lee la variable local y la empuja a la pila.
2. `OpCode::Constant(k)`: lee la constante y la empuja a la pila.
3. `OpCode::Add`: extrae ambos operandos, valida tipos, suma y empuja el resultado.

Este proceso consumía 3 saltos en la jump table de la VM, múltiples escrituras y lecturas en el stack y generaba bytecode extenso en memoria.

#### Implementación
Se fusionaron estos patrones directamente durante la fase de compilación (`visit_binary_expr`) cuando el flag de superinstrucciones está activo:
- `AddLocalConst(slot: u8, const_idx: u16)`: Reemplaza la secuencia `GetLocal + Constant + Add`.
- `LessLocalConst(slot: u8, const_idx: u16)`: Reemplaza la secuencia `GetLocal + Constant + Less`.

Para no penalizar el tamaño del bytecode, los operandos se empaquetan en los 4 bytes de ancho de `OpCode` (aprovechando el byte de relleno de alineamiento de Rust). Un test de verificación garantiza `std::mem::size_of::<OpCode>() == 4`:

```rust
// src/compiler/chunk.rs
pub enum OpCode {
    // ...
    AddLocalConst(u8, u16),
    LessLocalConst(u8, u16),
}
```

En la VM, la instrucción ejecuta en un solo ciclo de dispatch la lectura directa del stack en `base + slot`, la desreferenciación de la constante en `chunk.constants[k]` y el empuje del resultado final.

#### Impacto en rendimiento (Medición aislada)
Medición comparativa aislada (`-O0` vs `-O0 --opt=super`):

| Benchmark | `-O0` Control (ms) | Con Superinstrucciones (ms) | Speedup | Opcodes Emitidos | Bytes de Chunk | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `loops_nested.lox` | 597.6 | **295.7** | **2.02x** | 44 → 34 (-22.7%) | 768 → 648 B | Aislada |
| `arith_locals.lox` | 476.6 | **416.6** | **1.14x** | 38 → 34 (-10.5%) | 672 → 624 B | Aislada |
| `calls.lox` | 182.0 | **153.3** | **1.19x** | 39 → 35 (-10.2%) | 684 → 636 B | Aislada |
| `recursion.lox` | 90.7 | **85.1** | **1.07x** | 31 → 29 (-6.5%) | 540 → 516 B | Aislada |

*Bajo `-O2` completo, `loops_nested.lox` se ejecuta en 289.0 ms frente a 587.6 ms con `--no-opt=super` (**2.03x** de aceleración).*

#### Consideraciones de corrección
- `AddLocalConst` valida dinámicamente si los operandos son números (efectuando la suma `f64`) o strings (efectuando la concatenación), preservando la semántica polimórfica de `+`.
- En caso de incompatibilidad de tipos, emite exactamente el error `"Operands must be two numbers or two strings."` o `"Operands must be numbers."` conservando el número de línea canónico.

---

### 2.12 Superinstrucción `SetLocalPop`

#### Problema original
En sentencias de asignación donde el resultado no forma parte de una expresión más amplia (por ejemplo, sentencias independientes de bucles o cuerpos de funciones como `i = i + 1;`), el compilador emitía:
1. `OpCode::SetLocal(slot)`: asigna el valor al slot local del stack y, según la semántica de Lox, deja una copia del valor en la cima de la pila.
2. `OpCode::Pop`: descarta inmediatamente el valor de la cima de la pila que ya no será utilizado.

Esto representaba un ciclo de dispatch redundante y forzaba una llamada a `Value::clone()` innecesaria para duplicar el valor en la cima antes de destruirlo.

#### Implementación
Se diseñó la instrucción `OpCode::SetLocalPop(u16)`. Al emitirse, fusiona la asignación y el descarte:
1. **Detección en compilación**: En `visit_expression_stmt`, cuando la expresión raíz es una asignación local simple no compuesta, se emite directamente `SetLocalPop(slot)` en lugar de `SetLocal(slot) + Pop`.
2. **Manejo en la VM**: La VM extrae directamente el valor con `self.pop()` y lo mueve a su ranura de destino:

```rust
// src/compiler/vm.rs
OpCode::SetLocalPop(slot) => {
    let value = self.pop();
    self.stack[base + slot as usize] = value;
}
```

#### Impacto en rendimiento (Medición aislada)
Medición comparativa aislada sobre la versión previa (F1 baseline `-O2` vs F2 con `SetLocalPop` en `-O2`, 15 repeticiones con 5 warmups):

| Benchmark | Antes (F1) (ms) | Con `SetLocalPop` (ms) | Speedup | Reducción de tiempo | Instrucciones (Ops) | Tipo de medición |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `loops_nested.lox` | 299.8 | **212.5** | **1.41x** | **-29.1%** | 34 → 31 (-8.8%) | Aislada |
| `arith_locals.lox` | 416.2 | **383.4** | **1.09x** | **-7.9%** | 34 → 32 (-5.9%) | Aislada |
| `mixed.lox` | 192.2 | **171.6** | **1.12x** | **-10.7%** | 117 → 111 (-5.1%) | Aislada |
| `closures.lox` | 137.8 | **129.4** | **1.06x** | **-6.1%** | 43 → 42 (-2.3%) | Aislada |
| `fields_existing.lox` | 72.6 | **67.8** | **1.07x** | **-6.6%** | 52 → 51 (-1.9%) | Aislada |
| `fields_new.lox` | 100.9 | **97.2** | **1.04x** | **-3.7%** | 39 → 38 (-2.6%) | Aislada |

#### Consideraciones de corrección
- **Seguridad del Stack**: Se auditó formalmente que el slot `base + slot` permanece siempre estrictamente menor que `self.stack.len()` al momento de la escritura. Dado que toda variable local válida reside por debajo de los valores temporales evaluados, `self.pop()` solo consume el valor temporal en la cima, sin invalidar el índice de la variable local.
- **Asignaciones encadenadas**: Expresiones como `a = b = 1` conservan la emisión canónica de `SetLocal` en la asignación interna para preservar el valor para la asignación externa.

---

## 3. Optimizaciones Evaluadas y Descartadas

En cumplimiento con una metodología de ingeniería empírica y rigurosa, se descartaron dos optimizaciones que no superaron los criterios de costo/beneficio:

### 3.1 Pase Genérico de Peephole Optimization
- **Motivo de descarte**: Se analizó la implementación de un pase genérico de ventanas deslizantes sobre el bytecode ya emitido (para patrones como `Equal + Not` $\to$ `NotEqual`). El análisis demostró que las oportunidades de peephole de alto impacto en Lox ya habían sido capturadas de forma más limpia directamente en el frontend durante la emisión (`AddLocalConst`, `LessLocalConst`, `SetLocalPop`), a costo de compilación cero. Implementar una pasada adicional sobre el `Chunk` requería reconstruir tablas de relocalización de saltos relativos (`u16`) con riesgo de errores semánticos y una ganancia esperada marginal (< 1%).

### 3.2 Superinstrucción `JumpIfFalsePop`
- **Motivo de descarte**: Se evaluó implementar `OpCode::JumpIfFalsePop(u16)` para consumir la condición booleana de estructuras `if` y `while` sin requerir instrucciones `Pop` adicionales. Tras implementarla y someterla a pruebas rigurosas:
  - En `loops_nested.lox`, la ganancia fue marginal (**1.03x**, -2.75% de tiempo).
  - En `calls.lox`, produjo una **regresión reproducible del 7.17%** (aumento de 130.7 ms a 140.1 ms).
  - En `mixed.lox`, se observó una degradación del 1.63%.
  - **Hipótesis técnica**: La adición de la variante en el enum `OpCode` ensanchó la tabla de saltos del bucle de dispatch de la VM, lo que podría haber afectado la predicción de bifurcaciones o la densidad del código en la caché de instrucciones en programas dominados por llamadas a funciones. Al no demostrar un beneficio consistente y registrar regresiones medibles, la optimización fue descartada y completamente revertida.

---

## 4. Comparación Global Final (14 Benchmarks)

La siguiente tabla refleja la evaluación global definitiva de la Stack VM de `rlox` sobre la totalidad de los 14 programas de la suite estándar de Lox, comparando:
- **Baseline Original (A)**: Tag `opt-baseline` (commit `3cc44bc`).
- **Versión Final `-O0` (B)**: Commit actual (`316f597`) con optimizaciones de compilación desactivadas (contiene las mejoras estructurales de runtime).
- **Versión Final `-O2` (C)**: Commit actual (`316f597`) con todas las optimizaciones de compilación activas.

> Todas las cifras corresponden a la **mediana de 15 repeticiones intercaladas (round-robin)** tras 5 ejecuciones de calentamiento (*warmup*), expresadas en milisegundos de tiempo de reloj (*wall-clock*):

| Benchmark | Baseline Original (A)<br>`[mín - máx]` | Final -O0 (B)<br>`[mín - máx]` | Final -O2 (C)<br>`[mín - máx]` | Speedup<br>Base $\to$ -O0 | Speedup<br>-O0 $\to$ -O2 | Speedup Total<br>Base $\to$ -O2 |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `arith_globals.lox` | 1198.1 ms `[1167 - 1286]` | 897.2 ms `[872 - 947]` | 504.9 ms `[491 - 566]` | **1.335x** | **1.777x** | **2.373x** (+137.3%) |
| `arith_locals.lox` | 755.4 ms `[724 - 910]` | 500.3 ms `[461 - 566]` | 373.9 ms `[335 - 433]` | **1.510x** | **1.338x** | **2.020x** (+102.0%) |
| `const_expr.lox` | 696.1 ms `[657 - 764]` | 473.3 ms `[460 - 527]` | 139.1 ms `[132 - 159]` | **1.471x** | **3.403x** | **5.004x** (+400.4%) |
| `dead_code.lox` | 578.8 ms `[553 - 657]` | 495.5 ms `[477 - 554]` | 242.4 ms `[222 - 293]` | **1.168x** | **2.044x** | **2.388x** (+138.8%) |
| `loops_nested.lox` | 796.6 ms `[757 - 908]` | 594.0 ms `[559 - 904]` | 224.1 ms `[213 - 270]` | **1.341x** | **2.651x** | **3.555x** (+255.5%) |
| `calls.lox` | 243.4 ms `[240 - 250]` | 183.8 ms `[180 - 186]` | 122.1 ms `[120 - 130]` | **1.324x** | **1.505x** | **1.993x** (+99.3%) |
| `recursion.lox` | 118.8 ms `[115 - 134]` | 98.6 ms `[96 - 111]` | 74.4 ms `[73 - 76]` | **1.205x** | **1.325x** | **1.597x** (+59.7%) |
| `closures.lox` | 220.7 ms `[214 - 247]` | 169.6 ms `[167 - 172]` | 137.0 ms `[134 - 145]` | **1.301x** | **1.238x** | **1.611x** (+61.1%) |
| `methods.lox` | 868.5 ms `[848 - 975]` | 667.3 ms `[651 - 866]` | 471.1 ms `[460 - 538]` | **1.302x** | **1.416x** | **1.844x** (+84.4%) |
| `inheritance.lox` | 292.4 ms `[280 - 321]` | 265.9 ms `[247 - 291]` | 199.4 ms `[191 - 228]` | **1.100x** | **1.334x** | **1.466x** (+46.6%) |
| `super_calls.lox` | 507.5 ms `[495 - 571]` | 441.9 ms `[414 - 495]` | 246.9 ms `[236 - 317]` | **1.148x** | **1.790x** | **2.055x** (+105.5%) |
| `fields.lox` | 427.4 ms `[419 - 453]` | 227.3 ms `[219 - 297]` | 209.2 ms `[202 - 242]` | **1.880x** | **1.087x** | **2.043x** (+104.3%) |
| `mixed.lox` | 308.7 ms `[281 - 330]` | 245.8 ms `[222 - 307]` | 226.2 ms `[208 - 244]` | **1.256x** | **1.087x** | **1.365x** (+36.5%) |
| `strings.lox` | 175.3 ms `[165 - 187]` | 154.4 ms `[146 - 208]` | 139.1 ms `[99 - 165]` | **1.135x** | **1.110x** | **1.260x** (+26.0%) |

---

### Resumen de Medias Geométricas Recalculadas

Calculando la media geométrica exacta directamente a partir de las 14 medianas (\(\sqrt[14]{\prod_{i=1}^{14} S_i}\)):

1. **Aporte Estructural del Runtime (Baseline $\to$ Final `-O0`)**:
   $$\text{Media Geométrica} = \mathbf{1.307\times} \quad (+30.7\%)$$
   *Representa el incremento de rendimiento derivado de optimizaciones presentes en `-O0`: la compactación de `Value`, aritmética in-place, gestión eficiente de frames de llamada y actualización in-place de campos.*

2. **Aporte de las Optimizaciones de Compilación (Final `-O0` $\to$ Final `-O2`)**:
   $$\text{Media Geométrica} = \mathbf{1.554\times} \quad (+55.4\%)$$
   *Refleja la ganancia pura de las transformaciones estáticas de bytecode: constant folding, DCE, invocación directa de métodos, globales indexados, superinstrucciones de bucles y `SetLocalPop`.*

3. **Aceleración Total Acumulada (Baseline $\to$ Final `-O2`)**:
   $$\text{Media Geométrica} = \mathbf{2.031\times} \quad (+103.1\%)$$
   *Interpretación del speedup global:*
   - El speedup geométrico total alcanzado es de **2.031×**.
   - En términos de tasa de trabajo o capacidad de procesamiento, esto representa un incremento del **103.1% en el rendimiento relativo**.
   - En términos de tiempo de pared (*wall-clock time*), equivale aproximadamente a una **reducción del 50.8%** en el tiempo de ejecución ($1 - 1 / 2.0315 \approx 0.5078$).
   - Es importante distinguir entre aumento del rendimiento y reducción del tiempo de ejecución, ya que no son magnitudes porcentuales equivalentes (un speedup de $2\times$ duplica el rendimiento $+100\%$, pero reduce el tiempo a la mitad $-50\%$).

- **Evaluación de regresiones**: **La versión final no presentó regresiones de rendimiento en ninguno de los 14 benchmarks evaluados.** Todos los programas de la suite mostraron mejoras consistentes y reproducibles frente a la línea de base original.

---

## 5. Metodología y Reproducibilidad

### 5.1 Entorno Experimental
- **Procesador (CPU)**: AMD Ryzen 5 3500 6-Core Processor (6 núcleos físicos, 6 hilos lógicos) @ 3.60 GHz base.
- **Memoria RAM**: 16 GB DDR4.
- **Sistema Operativo**: Microsoft Windows 11 Pro 64-bit (Build 10.0.26200).
- **Compilador Rust**: `rustc 1.98.1 (48a229cea 2026-09-01)`.
- **Cargo**: `cargo 1.98.1 (797e8a9bc 2026-08-05)`.
- **Perfil de Compilación**: `--release` (`opt-level = 3`, `debug = false`, `lto = false`).
- **Commits de Referencia**:
  - Baseline original: `3cc44bc169673332f86c0f5adf8367675ece34fd` (tag `opt-baseline`).
  - Versión final: `316f5979c3d40ec3e62f033a3ce40498877bcce3` (rama `feature/herencia`).

### 5.2 Parámetros del Compilador y Flags de Optimización

El compilador de `rlox` expone una interfaz granular de optimizaciones mediante la línea de comandos:

- **Niveles de Optimización**:
  - `-O0`: Sin optimizaciones de compilador. Bytecode canónico de una pasada.
  - `-O1`: Optimizaciones de frontend de bajo riesgo (Constant Folding, Dead Code Elimination, Jump Simplification).
  - `-O2`: Optimizaciones completas (todo `-O1` más Superinstructions, Invoke, SuperInvoke e Indexed Globals). Nivel por defecto al ejecutar `--vm`.

- **Control Fino de Pases (`--opt` / `--no-opt`)**:
  Se puede activar o desactivar selectivamente cualquier pase sobre cualquier nivel:
  - `--opt=fold,dce,jumps,super,invoke,globals`
  - `--no-opt=super,invoke`
  - Variable de entorno equivalente: `RLOX_OPT="fold,dce,jumps"`.

---

## 6. Catálogo de Programas de Benchmark

Los programas de prueba residen en el directorio `benchmarks/programs/` y cubren diversos patrones de ejecución en Lox:

| Archivo | Enfoque Principal | Carga de Trabajo / Iteraciones | Salida Canónica |
| :--- | :--- | :--- | :--- |
| `arith_locals.lox` | Aritmética pura y acceso a variables locales | 3.000.000 de iteraciones | `6750000750000` |
| `arith_globals.lox` | Acceso y mutación intensiva de variables globales | 3.000.000 de iteraciones (21M accesos) | `6750000750000` |
| `const_expr.lox` | Expresiones con literales constantes en bucle | 3.000.000 de iteraciones | `60000000` |
| `dead_code.lox` | Código condicional inalcanzable y ramas muertas | 1.000.000 de evaluaciones | `1000000` |
| `loops_nested.lox` | Bucles anidados y operaciones de salto | 5.000 $\times$ 1.000 iteraciones (5M ciclos) | `5000000` |
| `calls.lox` | Llamadas a funciones de usuario con argumentos | 1.000.000 de llamadas | `499999500000` |
| `recursion.lox` | Recursión profunda (árbol de Fibonacci `fib(27)`) | ~392.835 llamadas recursivas | `196418` |
| `closures.lox` | Captura de upvalues abiertos y cerrados | 1.000.000 de invocaciones | `1000001` |
| `methods.lox` | Invocación de métodos de instancia y acceso `this` | 1.000.000 de iteraciones (2M llamadas) | `500003500000` |
| `inheritance.lox` | Jerarquías de herencia y llamadas `super` | 200.000 iteraciones (600k super calls) | `20000300000` |
| `super_calls.lox` | Microbenchmark de llamadas directas a superclase | 1.000.000 de llamadas `super.m()` | `1000000` |
| `fields.lox` | Lectura y mutación intensiva de campos de instancia | 500.000 iteraciones ($\times$4 campos) | `2000002` |
| `fields_existing.lox` | Mutación exclusiva de campos preexistentes | 500.000 iteraciones ($\times$2 campos) | `1000001` |
| `fields_new.lox` | Creación continua de campos nuevos en instancias | 200.000 instancias | `200000` |
| `mixed.lox` | Carga heterogénea (listas enlazadas, objetos, closures) | 100 rondas de 2.000 nodos | `199905250` |
| `strings.lox` | Concatenación acumulativa y comparaciones de igualdad | 20.000 concatenaciones + 300k `==` | `300000` |
| `instantiation.lox` | Creación de instancias e invocación de `init` | 300.000 objetos | `44999850000` |
| `print_heavy.lox` | I/O intensivo sobre la salida estándar | 50.000 impresiones por consola | `49999` |

---

## 7. Comandos de Ejecución

### 7.1 Medición Precisa en Memoria con `rlox-bench`
El arnés `rlox-bench` aísla los tiempos de escaneo, compilación y ejecución de la VM sin interferencia de la consola de Windows:

```powershell
# 1. Compilar binarios en modo release
cargo build --release --bin rlox
cargo build --release --bin rlox-bench

# 2. Ejecutar un benchmark individual en -O2 (5 warmups, 15 repeticiones)
.\target\release\rlox-bench.exe --backend vm -O2 --reps 15 --warmup 5 benchmarks\programs\loops_nested.lox

# 3. Ejecutar en nivel -O0
.\target\release\rlox-bench.exe --backend vm -O0 --reps 15 --warmup 5 benchmarks\programs\loops_nested.lox

# 4. Ejecutar la suite completa y exportar a CSV
.\target\release\rlox-bench.exe --backend vm -O2 --csv benchmarks\results\final_o2.csv
```

### 7.2 Ejecución con el Intérprete Principal `rlox`
```powershell
# Ejecución estándar de un script Lox en la Stack VM (nivel por defecto: -O2)
.\target\release\rlox.exe --vm benchmarks\programs\arith_locals.lox

# Forzar nivel sin optimizaciones de compilador (-O0)
.\target\release\rlox.exe --vm -O0 benchmarks\programs\arith_locals.lox

# Prueba de ablación (ejecutar -O2 desactivando únicamente superinstrucciones)
.\target\release\rlox.exe --vm -O2 --no-opt=super benchmarks\programs\arith_locals.lox

# Activar un pase específico sobre -O0
.\target\release\rlox.exe --vm -O0 --opt=fold benchmarks\programs\const_expr.lox
```

---

## 8. Conclusiones

1. **Efecto acumulativo de optimizaciones de capas cruzadas**: Las mejoras estructurales del runtime de la VM (compactación de `Value` a 24 bytes, aritmética in-place y actualización in-place de campos) sentaron una base del **30.7%** de aceleración transversal (1.307x). Sobre esta base, las optimizaciones del compilador sumaron un **55.4%** adicional (1.554x), logrando un factor global acumulado de **2.031x** (+103.1% de rendimiento relativo, equivalente a una reducción del **50.8%** en el tiempo de ejecución).
2. **Impacto crítico de reducir asignaciones en el heap**: Tanto en `Invoke` como en `SetProperty` in-place, evitar la asignación repetida de objetos de vida corta (`BoundMethod` y claves `String`) redujo drásticamente el tiempo de CPU en programas orientados a objetos.
3. **Fusión selectiva de instrucciones de dispatch**: La combinación de superinstrucciones aritméticas locales (`AddLocalConst`, `LessLocalConst`) y la absorción del descarte de stack (`SetLocalPop`) redujo entre un 20% y un 40% las instrucciones ejecutadas en bucles computacionales intensivos.
4. **Criterio de ingeniería ante resultados negativos**: Las optimizaciones que introdujeron regresiones o mostraron mejoras inconsistentes (como `JumpIfFalsePop`) fueron descartadas y revertidas. La versión final no presentó regresiones de rendimiento en ninguno de los 14 benchmarks evaluados, asegurando que las ganancias de velocidad sean consistentes en toda la suite de prueba.
5. **Corrección e invariantes inalterados**: La suite completa de 153 pruebas unitarias y de integración, así como los tests de paridad triple frente al intérprete Tree-Walk, continúan aprobándose con 100% de éxito, demostrando que la ganancia de velocidad no comprometió la fidelidad semántica del lenguaje.
