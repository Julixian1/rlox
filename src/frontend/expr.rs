use crate::token::Token;
use std::fmt;

/// Trait patrón Visitor para recorrer y procesar expresiones del AST.
pub trait ExprVisitor<R> {
    /// Procesa una expresión literal.
    fn visit_literal_expr(&mut self, value: &LiteralValue) -> R;
    /// Procesa una expresión unaria (ej. `-x`, `!y`).
    fn visit_unary_expr(&mut self, operator: &Token, right: &Expr) -> R;
    /// Procesa una expresión binaria (ej. `a + b`, `x == y`).
    fn visit_binary_expr(&mut self, left: &Expr, operator: &Token, right: &Expr) -> R;
    /// Procesa una expresión agrupada entre paréntesis `(expr)`.
    fn visit_grouping_expr(&mut self, expression: &Expr) -> R;
    /// Procesa el acceso a una variable por su identificador.
    fn visit_variable_expr(&mut self, name: &Token) -> R;
    /// Procesa una asignación de variable (`x = expr`).
    fn visit_assign_expr(&mut self, name: &Token, value: &Expr) -> R;
    /// Procesa una expresión lógica de cortocircuito (`or`, `and`).
    fn visit_logical_expr(&mut self, left: &Expr, operator: &Token, right: &Expr) -> R;
    /// Procesa una llamada a función (`fun_name(arg1, arg2)`).
    fn visit_call_expr(&mut self, callee: &Expr, paren: &Token, arguments: &[Expr]) -> R;
    /// Procesa el acceso a una propiedad (`object.name`).
    fn visit_get_expr(&mut self, object: &Expr, name: &Token) -> R;
    /// Procesa la asignación a una propiedad (`object.name = value`).
    fn visit_set_expr(&mut self, object: &Expr, name: &Token, value: &Expr) -> R;
    /// Procesa la expresión `this`.
    fn visit_this_expr(&mut self, expr: &Expr, keyword: &Token) -> R;
    /// Procesa el acceso a método de superclase (`super.method`).
    fn visit_super_expr(&mut self, expr: &Expr, keyword: &Token, method: &Token) -> R;
}

// Valores literales en el AST (distinto a token::Literal del scanner).
#[derive(Debug, Clone, PartialEq)]
pub enum LiteralValue {
    Number(f64),
    StringVal(String),
    Bool(bool),
    Nil,
}

impl fmt::Display for LiteralValue {
    /// Formatea el valor literal a una representación en cadena de caracteres.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiteralValue::Number(n) => write!(f, "{}", n),
            LiteralValue::StringVal(s) => write!(f, "\"{}\"", s),
            LiteralValue::Bool(b) => write!(f, "{}", b),
            LiteralValue::Nil => write!(f, "nil"),
        }
    }
}

// Nodos de expresión del AST.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal {
        value: LiteralValue,
    },
    Unary {
        operator: Token,
        right: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: Token,
        right: Box<Expr>,
    },
    Grouping {
        expression: Box<Expr>,
    },
    Variable {
        name: Token,
    },
    Assign {
        name: Token,
        value: Box<Expr>,
    },
    Logical {
        left: Box<Expr>,
        operator: Token,
        right: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        paren: Token,
        arguments: Vec<Expr>,
    },
    Get {
        object: Box<Expr>,
        name: Token,
    },
    Set {
        object: Box<Expr>,
        name: Token,
        value: Box<Expr>,
    },
    This {
        keyword: Token,
    },
    Super {
        keyword: Token,
        method: Token,
    },
}

impl Expr {
    /// Acepta un visitante implementando el patrón Visitor para despachar el método `visit_*` correspondiente según el tipo de nodo expresión.
    pub fn accept<R>(&self, visitor: &mut impl ExprVisitor<R>) -> R {
        match self {
            Expr::Literal { value } => visitor.visit_literal_expr(value),
            Expr::Unary { operator, right } => visitor.visit_unary_expr(operator, right),
            Expr::Binary {
                left,
                operator,
                right,
            } => visitor.visit_binary_expr(left, operator, right),
            Expr::Grouping { expression } => visitor.visit_grouping_expr(expression),
            Expr::Variable { name } => visitor.visit_variable_expr(name),
            Expr::Assign { name, value } => visitor.visit_assign_expr(name, value),
            Expr::Logical {
                left,
                operator,
                right,
            } => visitor.visit_logical_expr(left, operator, right),
            Expr::Call {
                callee,
                paren,
                arguments,
            } => visitor.visit_call_expr(callee, paren, &arguments),
            Expr::Get { object, name } => visitor.visit_get_expr(object, name),
            Expr::Set {
                object,
                name,
                value,
            } => visitor.visit_set_expr(object, name, value),
            Expr::This { keyword } => visitor.visit_this_expr(self, keyword),
            Expr::Super { keyword, method } => visitor.visit_super_expr(self, keyword, method),
        }
    }
}

impl fmt::Display for Expr {
    /// Formatea el árbol de expresiones a formato prefijo (S-expressions) para inspección y depuración.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Literal { value } => write!(f, "{}", value),
            Expr::Unary { operator, right } => {
                write!(f, "({} {})", operator.lexeme, right)
            }
            Expr::Binary {
                left,
                operator,
                right,
            } => {
                write!(f, "({} {} {})", operator.lexeme, left, right)
            }
            Expr::Grouping { expression } => {
                write!(f, "(group {})", expression)
            }
            Expr::Variable { name } => write!(f, "{}", name.lexeme),
            Expr::Assign { name, value } => {
                write!(f, "(= {} {})", name.lexeme, value)
            }
            Expr::Logical {
                left,
                operator,
                right,
            } => {
                write!(f, "({} {} {})", operator.lexeme, left, right)
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                write!(f, "(call {}", callee)?;
                for arg in arguments {
                    write!(f, " {}", arg)?;
                }
                write!(f, ")")
            }
            Expr::Get { object, name } => {
                write!(f, "(. {} {})", object, name.lexeme)
            }
            Expr::Set {
                object,
                name,
                value,
            } => {
                write!(f, "(= (. {} {}) {})", object, name.lexeme, value)
            }
            Expr::This { .. } => write!(f, "this"),
            Expr::Super { method, .. } => write!(f, "(super {})", method.lexeme),
        }
    }
}
