use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::environment::Environment;
use crate::interpreter::Interpreter;
use crate::stmt::Stmt;
use crate::token::Token;
use crate::value::Value;

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
    pub name: Token,
    pub params: Vec<Token>,
    pub body: Rc<Vec<Stmt>>,
    pub closure: Rc<RefCell<Environment>>,
}

impl LoxFunction {
    /// Crea una nueva instancia de función de Lox asociada a su entorno de clausura.
    pub fn new(
        name: Token,
        params: Vec<Token>,
        body: Rc<Vec<Stmt>>,
        closure: Rc<RefCell<Environment>>,
    ) -> Self {
        LoxFunction {
            name,
            params,
            body,
            closure,
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
            Err(ReturnSignal::Return(value)) => Ok(value),
            Err(ReturnSignal::Error(err)) => Err(ReturnSignal::Error(err)),
            Ok(_) => Ok(Value::Nil),
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
    /// Compara dos funciones por igualdad basándose en sus nombres.
    fn eq(&self, other: &Self) -> bool {
        self.name.lexeme == other.name.lexeme
    }
}
