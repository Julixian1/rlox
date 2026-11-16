//! Nodos de expresión del AST y trait visitor para su procesamiento.

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
    /// Procesa una asignación compuesta de variable (`x += expr`, etc.).
    fn visit_assign_modify_expr(&mut self, name: &Token, operator: &Token, value: &Expr) -> R;
    /// Procesa una expresión lógica de cortocircuito (`or`, `and`).
    fn visit_logical_expr(&mut self, left: &Expr, operator: &Token, right: &Expr) -> R;
    /// Procesa una llamada a función (`fun_name(arg1, arg2)`).
    fn visit_call_expr(&mut self, callee: &Expr, paren: &Token, arguments: &[Expr]) -> R;
    /// Procesa el acceso a una propiedad (`object.name`).
    fn visit_get_expr(&mut self, object: &Expr, name: &Token) -> R;
    /// Procesa la asignación a una propiedad (`object.name = value`).
    fn visit_set_expr(&mut self, object: &Expr, name: &Token, value: &Expr) -> R;
    /// Procesa una asignación compuesta a una propiedad (`object.name += value`, etc.).
    fn visit_set_modify_expr(
        &mut self,
        object: &Expr,
        name: &Token,
        operator: &Token,
        value: &Expr,
    ) -> R;
    /// Procesa la expresión `this`.
    fn visit_this_expr(&mut self, expr: &Expr, keyword: &Token) -> R;
    /// Procesa el acceso a método de superclase (`super.method`).
    fn visit_super_expr(&mut self, expr: &Expr, keyword: &Token, method: &Token) -> R;
    /// Procesa una expresión condicional ternaria (`condition ? then_branch : else_branch`).
    fn visit_ternary_expr(&mut self, condition: &Expr, then_branch: &Expr, else_branch: &Expr)
    -> R;
    /// Procesa una lista literal (`[a, b, c]`).
    fn visit_list_expr(&mut self, bracket: &Token, elements: &[Expr]) -> R;
    /// Procesa el acceso por índice (`object[index]`).
    fn visit_index_get_expr(&mut self, object: &Expr, bracket: &Token, index: &Expr) -> R;
    /// Procesa la asignación por índice (`object[index] = value`).
    fn visit_index_set_expr(
        &mut self,
        object: &Expr,
        bracket: &Token,
        index: &Expr,
        value: &Expr,
    ) -> R;
    /// Procesa una asignación compuesta por índice (`object[index] += value`, etc.).
    fn visit_index_set_modify_expr(
        &mut self,
        object: &Expr,
        bracket: &Token,
        index: &Expr,
        operator: &Token,
        value: &Expr,
    ) -> R;
}

/// Representa los valores literales presentes en los nodos del AST.
#[derive(Debug, Clone, PartialEq)]
pub enum LiteralValue {
    /// Literal numérico en coma flotante.
    Number(f64),
    /// Literal de cadena de texto.
    StringVal(String),
    /// Literal booleano.
    Bool(bool),
    /// Literal nulo `nil`.
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

/// Nodos del Árbol de Sintaxis Abstracta (AST) que representan expresiones evaluables.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Expresión literal constante.
    Literal { value: LiteralValue },
    /// Operación unaria prefija (`-x`, `!y`).
    Unary { operator: Token, right: Box<Expr> },
    /// Operación binaria infija (`a + b`, `x == y`).
    Binary {
        left: Box<Expr>,
        operator: Token,
        right: Box<Expr>,
    },
    /// Expresión agrupada entre paréntesis `(expr)`.
    Grouping { expression: Box<Expr> },
    /// Acceso a una variable por identificador.
    Variable { name: Token },
    /// Asignación a una variable (`x = expr`).
    Assign { name: Token, value: Box<Expr> },
    /// Asignación compuesta a una variable (`x += expr`, etc.).
    AssignModify {
        name: Token,
        operator: Token,
        value: Box<Expr>,
    },
    /// Operador lógico con evaluación de cortocircuito (`and`, `or`).
    Logical {
        left: Box<Expr>,
        operator: Token,
        right: Box<Expr>,
    },
    /// Llamada a función o método (`callee(arguments)`).
    Call {
        callee: Box<Expr>,
        paren: Token,
        arguments: Vec<Expr>,
    },
    /// Acceso a propiedad de instancia (`object.name`).
    Get { object: Box<Expr>, name: Token },
    /// Asignación a propiedad de instancia (`object.name = value`).
    Set {
        object: Box<Expr>,
        name: Token,
        value: Box<Expr>,
    },
    /// Asignación compuesta a propiedad de instancia (`object.name += value`, etc.).
    SetModify {
        object: Box<Expr>,
        name: Token,
        operator: Token,
        value: Box<Expr>,
    },
    /// Referencia a la instancia receptora actual (`this`).
    This { keyword: Token },
    /// Acceso a método de la superclase (`super.method`).
    Super { keyword: Token, method: Token },
    /// Expresión condicional ternaria (`condition ? then_branch : else_branch`).
    Ternary {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    /// Literal de lista (`[elem1, elem2, ...]`).
    List { bracket: Token, elements: Vec<Expr> },
    /// Acceso indexado a colección (`object[index]`).
    IndexGet {
        object: Box<Expr>,
        bracket: Token,
        index: Box<Expr>,
    },
    /// Asignación indexada a colección (`object[index] = value`).
    IndexSet {
        object: Box<Expr>,
        bracket: Token,
        index: Box<Expr>,
        value: Box<Expr>,
    },
    /// Asignación compuesta indexada (`object[index] += value`, etc.).
    IndexSetModify {
        object: Box<Expr>,
        bracket: Token,
        index: Box<Expr>,
        operator: Token,
        value: Box<Expr>,
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
            Expr::AssignModify {
                name,
                operator,
                value,
            } => visitor.visit_assign_modify_expr(name, operator, value),
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
            Expr::SetModify {
                object,
                name,
                operator,
                value,
            } => visitor.visit_set_modify_expr(object, name, operator, value),
            Expr::This { keyword } => visitor.visit_this_expr(self, keyword),
            Expr::Super { keyword, method } => visitor.visit_super_expr(self, keyword, method),
            Expr::Ternary {
                condition,
                then_branch,
                else_branch,
            } => visitor.visit_ternary_expr(condition, then_branch, else_branch),
            Expr::List { bracket, elements } => visitor.visit_list_expr(bracket, elements),
            Expr::IndexGet {
                object,
                bracket,
                index,
            } => visitor.visit_index_get_expr(object, bracket, index),
            Expr::IndexSet {
                object,
                bracket,
                index,
                value,
            } => visitor.visit_index_set_expr(object, bracket, index, value),
            Expr::IndexSetModify {
                object,
                bracket,
                index,
                operator,
                value,
            } => visitor.visit_index_set_modify_expr(object, bracket, index, operator, value),
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
            Expr::AssignModify {
                name,
                operator,
                value,
            } => {
                write!(f, "({} {} {})", operator.lexeme, name.lexeme, value)
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
            Expr::SetModify {
                object,
                name,
                operator,
                value,
            } => {
                write!(
                    f,
                    "({} (. {} {}) {})",
                    operator.lexeme, object, name.lexeme, value
                )
            }
            Expr::This { .. } => write!(f, "this"),
            Expr::Super { method, .. } => write!(f, "(super {})", method.lexeme),
            Expr::Ternary {
                condition,
                then_branch,
                else_branch,
            } => write!(f, "(?: {} {} {})", condition, then_branch, else_branch),
            Expr::List { elements, .. } => {
                write!(f, "(list")?;
                for el in elements {
                    write!(f, " {}", el)?;
                }
                write!(f, ")")
            }
            Expr::IndexGet { object, index, .. } => {
                write!(f, "([] {} {})", object, index)
            }
            Expr::IndexSet {
                object,
                index,
                value,
                ..
            } => {
                write!(f, "(= ([] {} {}) {})", object, index, value)
            }
            Expr::IndexSetModify {
                object,
                index,
                operator,
                value,
                ..
            } => {
                write!(
                    f,
                    "({} ([] {} {}) {})",
                    operator.lexeme, object, index, value
                )
            }
        }
    }
}
