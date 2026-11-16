//! Representación de funciones definidas por el usuario y señales de control para el intérprete Tree-Walk.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::class::LoxInstance;
use crate::environment::Environment;
use crate::interpreter::Interpreter;
use crate::stmt::Stmt;
use crate::token::Token;
use crate::value::Value;

static NEXT_FUNCTION_ID: AtomicUsize = AtomicUsize::new(0);

/// Señal de control utilizada para propagar valores de retorno, cortes de bucle o errores durante la ejecución.
#[derive(Debug)]
pub enum ReturnSignal {
    /// Retorno de un valor desde una función.
    Return(Value),
    /// Interrupción de un bucle (`break`).
    Break,
    /// Continuación a la siguiente iteración de un bucle (`continue`).
    Continue,
    /// Error en tiempo de ejecución con mensaje descriptivo.
    Error(String),
}

impl From<String> for ReturnSignal {
    /// Convierte un mensaje de error en formato `String` a un `ReturnSignal::Error`.
    fn from(err: String) -> Self {
        ReturnSignal::Error(err)
    }
}

/// Representa una función definida por el usuario en el lenguaje Lox (incluyendo su clausura/closure).
#[derive(Clone)]
pub struct LoxFunction {
    /// Identificador único de la función en tiempo de ejecución.
    pub id: usize,
    /// Token identificador con el nombre de la función.
    pub name: Token,
    /// Lista de tokens correspondientes a los parámetros formales.
    pub params: Vec<Token>,
    /// Sentencias que componen el cuerpo de la función.
    pub body: Rc<Vec<Stmt>>,
    /// Entorno léxico capturado al momento de declarar la función.
    pub closure: Rc<RefCell<Environment>>,
    /// Indica si la función actúa como inicializador (`init`) de una clase.
    pub is_initializer: bool,
}

impl LoxFunction {
    /// Crea una nueva instancia de función de Lox asociada a su entorno de clausura.
    pub fn new(
        name: Token,
        params: Vec<Token>,
        body: Rc<Vec<Stmt>>,
        closure: Rc<RefCell<Environment>>,
        is_initializer: bool,
    ) -> Self {
        LoxFunction {
            id: NEXT_FUNCTION_ID.fetch_add(1, Ordering::Relaxed),
            name,
            params,
            body,
            closure,
            is_initializer,
        }
    }

    /// Retorna el nombre de la función como una rebanada de texto (`&str`).
    pub fn name(&self) -> &str {
        &self.name.lexeme
    }

    /// Retorna la aridad de la función (cantidad de parámetros que espera recibir).
    pub fn arity(&self) -> usize {
        self.params.len()
    }

    /// Enlaza la función a una instancia específica, creando un entorno léxico que contiene 'this'.
    pub fn bind(&self, instance: Rc<RefCell<LoxInstance>>) -> LoxFunction {
        let environment = Rc::new(RefCell::new(Environment::new_enclosed(
            self.closure.clone(),
        )));
        environment
            .borrow_mut()
            .define("this".to_string(), Value::Instance(instance));
        LoxFunction {
            id: NEXT_FUNCTION_ID.fetch_add(1, Ordering::Relaxed),
            name: self.name.clone(),
            params: self.params.clone(),
            body: self.body.clone(),
            closure: environment,
            is_initializer: self.is_initializer,
        }
    }

    /// Invoca la función pasando los argumentos correspondientes y ejecutando el cuerpo dentro de un nuevo entorno cerrado (closure).
    pub fn call(
        &self,
        interpreter: &mut Interpreter,
        arguments: Vec<Value>,
    ) -> Result<Value, ReturnSignal> {
        let environment = Rc::new(RefCell::new(Environment::new_enclosed(
            self.closure.clone(),
        )));

        for (param, arg) in self.params.iter().zip(arguments) {
            environment.borrow_mut().define(param.lexeme.clone(), arg);
        }

        let result = interpreter.execute_block(&self.body, environment);

        match result {
            // Un error en tiempo de ejecución se propaga de inmediato sin convertirlo en 'this'.
            Err(ReturnSignal::Error(err)) => Err(ReturnSignal::Error(err)),
            Err(ReturnSignal::Break) => Err(ReturnSignal::Error(
                "Cannot break out of a function.".to_string(),
            )),
            Err(ReturnSignal::Continue) => Err(ReturnSignal::Error(
                "Cannot continue out of a function.".to_string(),
            )),

            // Retorno explícito: en inicializadores devuelve 'this'.
            Err(ReturnSignal::Return(value)) => {
                if self.is_initializer {
                    self.closure
                        .borrow()
                        .get_at(0, "this")
                        .map_err(ReturnSignal::Error)
                } else {
                    Ok(value)
                }
            }

            // Finalización normal del cuerpo de la función.
            Ok(_) => {
                if self.is_initializer {
                    self.closure
                        .borrow()
                        .get_at(0, "this")
                        .map_err(ReturnSignal::Error)
                } else {
                    Ok(Value::Nil)
                }
            }
        }
    }
}

impl fmt::Debug for LoxFunction {
    /// Formatea la función para mostrar `<fn nombre>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<fn {}>", self.name.lexeme)
    }
}

impl PartialEq for LoxFunction {
    /// Compara dos funciones por identidad única (instancia en tiempo de ejecución).
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
