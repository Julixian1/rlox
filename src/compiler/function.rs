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

/// Señal de control utilizada para propagar valores de retorno o errores durante la ejecución de funciones.
#[derive(Debug)]
pub enum ReturnSignal {
    Return(Value),
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
    pub id: usize,
    pub name: Token,
    pub params: Vec<Token>,
    pub body: Rc<Vec<Stmt>>,
    pub closure: Rc<RefCell<Environment>>,
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
            // 1. Un error en tiempo de ejecución SIEMPRE se propaga de inmediato como Err.
            // NUNCA debe convertirse en retorno de 'this'.
            Err(ReturnSignal::Error(err)) => Err(ReturnSignal::Error(err)),

            // 2. Un retorno explícito en un inicializador devuelve 'this'.
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

            // 3. Finalización normal del bloque de código.
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
