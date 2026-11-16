//! Implementación del algoritmo de análisis sintáctico de Pratt para expresiones según precedencia.

use super::Parser;
use crate::expr::{Expr, LiteralValue};
use crate::token::{Literal, TokenType};

/// Niveles de precedencia de operadores para el parser de Pratt, ordenados de menor a mayor.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precedence {
    /// Nivel más bajo de precedencia.
    Lowest,
    /// Operador condicional ternario `?:`.
    Ternary,
    /// Operador lógico `or`.
    Or,
    /// Operador lógico `and`.
    And,
    /// Operadores de igualdad `==` y `!=`.
    Equality,
    /// Operadores de comparación `<`, `<=`, `>`, `>=`.
    Comparison,
    /// Términos de suma y resta `+`, `-`.
    Term,
    /// Factores de multiplicación, división y módulo `*`, `/`, `%`.
    Factor,
    /// Operadores unarios prefijos `!`, `-`.
    Unary,
    /// Llamadas a función y accesos por propiedad o índice `()`, `.`, `[]`.
    Call,
}

impl Precedence {
    /// Retorna el siguiente nivel de precedencia para imponer asociatividad a izquierda.
    fn next(self) -> Self {
        use Precedence::*;
        match self {
            Lowest => Ternary,
            Ternary => Or,
            Or => And,
            And => Equality,
            Equality => Comparison,
            Comparison => Term,
            Term => Factor,
            Factor => Unary,
            Unary | Call => Call,
        }
    }
}

type PrefixFn = fn(&mut Parser) -> Result<Expr, String>;
type InfixFn = fn(&mut Parser, Expr) -> Result<Expr, String>;

/// Regla de análisis sintáctico de Pratt que asocia funciones prefijas e infijas con un nivel de precedencia.
struct ParseRule {
    /// Función de análisis para cuando el token aparece en posición prefija.
    prefix: Option<PrefixFn>,
    /// Función de análisis para cuando el token aparece en posición infija.
    infix: Option<InfixFn>,
    /// Nivel de precedencia asignado al token.
    precedence: Precedence,
}

/// Construye una nueva regla de parseo de Pratt.
fn rule(prefix: Option<PrefixFn>, infix: Option<InfixFn>, precedence: Precedence) -> ParseRule {
    ParseRule {
        prefix,
        infix,
        precedence,
    }
}

/// Retorna la regla de análisis de Pratt correspondiente a un tipo de token léxico.
#[rustfmt::skip]
fn get_rule(t: &TokenType) -> ParseRule {
    use Precedence as P;
    use TokenType as T;
    //                                    prefix                      infix                    precedence
    match t {
        T::LeftParen                  => rule(Some(Parser::grouping), Some(Parser::finish_call), P::Call),
        T::LeftBracket                => rule(Some(Parser::list_literal), Some(Parser::finish_index), P::Call),
        T::RightBracket               => rule(None,                   None,                      P::Lowest),
        T::Dot                        => rule(None,                   Some(Parser::dot),         P::Call),
        T::Number | T::String         => rule(Some(Parser::value),    None,                      P::Lowest),
        T::Nil | T::True | T::False   => rule(Some(Parser::literal),  None,                      P::Lowest),
        T::Identifier                 => rule(Some(Parser::variable), None,                      P::Lowest),
        T::This                       => rule(Some(Parser::this_expr),  None,                    P::Lowest),
        T::Super                      => rule(Some(Parser::super_expr), None,                    P::Lowest),
        T::Minus                      => rule(Some(Parser::unary),    Some(Parser::binary),      P::Term),
        T::Plus                       => rule(None,                   Some(Parser::binary),      P::Term),
        T::Star | T::Slash | T::Percent
                                      => rule(None,                   Some(Parser::binary),      P::Factor),
        T::Bang                       => rule(Some(Parser::unary),    None,                      P::Lowest),
        T::BangEqual | T::EqualEqual  => rule(None,                   Some(Parser::binary),      P::Equality),
        T::Greater | T::GreaterEqual | T::Less | T::LessEqual
                                      => rule(None,                   Some(Parser::binary),      P::Comparison),
        T::And                        => rule(None,                   Some(Parser::logical),     P::And),
        T::Or                         => rule(None,                   Some(Parser::logical),     P::Or),
        T::Question                   => rule(None,                   Some(Parser::ternary),     P::Ternary),
        T::Colon                      => rule(None,                   None,                      P::Lowest),
        _                             => rule(None,                   None,                      P::Lowest),
    }
}

impl Parser {
    /// Parsea una expresión cuyos operadores tengan precedencia >= `min`.
    pub(super) fn parse_precedence(&mut self, min: Precedence) -> Result<Expr, String> {
        let Some(prefix) = get_rule(&self.peek().token_type).prefix else {
            let t = self.peek();
            return Err(format!(
                "[line {}] Error at '{}': Expected expression.",
                t.line, t.lexeme
            ));
        };
        self.advance();
        let mut left = prefix(self)?;

        while min <= get_rule(&self.peek().token_type).precedence {
            self.advance();
            let infix = get_rule(&self.previous().token_type)
                .infix
                .expect("token con precedencia pero sin función infix");
            left = infix(self, left)?;
        }
        Ok(left)
    }

    /// Parsea una expresión agrupada entre paréntesis.
    fn grouping(&mut self) -> Result<Expr, String> {
        let expr = self.expression()?;
        self.consume(&TokenType::RightParen, "Expected ')' after expression.")?;
        Ok(Expr::Grouping {
            expression: Box::new(expr),
        })
    }

    /// Parsea un literal numérico o una cadena de texto.
    fn value(&mut self) -> Result<Expr, String> {
        let token = self.previous().clone();
        match token.literal {
            Some(Literal::Number(n)) => Ok(Expr::Literal {
                value: LiteralValue::Number(n),
            }),
            Some(Literal::String(s)) => Ok(Expr::Literal {
                value: LiteralValue::StringVal(s),
            }),
            _ => Err(format!("[line {}] Error: Invalid literal.", token.line)),
        }
    }

    /// Parsea un literal booleano (`true`, `false`) o `nil`.
    fn literal(&mut self) -> Result<Expr, String> {
        let value = match self.previous().token_type {
            TokenType::True => LiteralValue::Bool(true),
            TokenType::False => LiteralValue::Bool(false),
            _ => LiteralValue::Nil,
        };
        Ok(Expr::Literal { value })
    }

    /// Parsea una referencia a variable por su identificador.
    fn variable(&mut self) -> Result<Expr, String> {
        Ok(Expr::Variable {
            name: self.previous().clone(),
        })
    }

    /// Parsea una referencia a la instancia receptora actual (`this`).
    fn this_expr(&mut self) -> Result<Expr, String> {
        Ok(Expr::This {
            keyword: self.previous().clone(),
        })
    }

    /// Parsea una referencia a método de la superclase (`super.method`).
    fn super_expr(&mut self) -> Result<Expr, String> {
        let keyword = self.previous().clone();
        self.consume(&TokenType::Dot, "Expected '.' after 'super'.")?;
        let method = self.consume(&TokenType::Identifier, "Expected superclass method name.")?;
        Ok(Expr::Super { keyword, method })
    }

    /// Parsea una operación unaria prefija (`-expr`, `!expr`).
    fn unary(&mut self) -> Result<Expr, String> {
        let operator = self.previous().clone();
        let right = self.parse_precedence(Precedence::Unary)?;
        Ok(Expr::Unary {
            operator,
            right: Box::new(right),
        })
    }

    /// Parsea una operación binaria infija asociativa a izquierda.
    fn binary(&mut self, left: Expr) -> Result<Expr, String> {
        let operator = self.previous().clone();
        let prec = get_rule(&operator.token_type).precedence;
        let right = self.parse_precedence(prec.next())?;
        Ok(Expr::Binary {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        })
    }

    /// Parsea una operación lógica infija (`and`, `or`).
    fn logical(&mut self, left: Expr) -> Result<Expr, String> {
        let operator = self.previous().clone();
        let prec = get_rule(&operator.token_type).precedence;
        let right = self.parse_precedence(prec.next())?;
        Ok(Expr::Logical {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        })
    }

    /// Parsea el acceso a una propiedad de objeto mediante operador punto (`object.prop`).
    fn dot(&mut self, left: Expr) -> Result<Expr, String> {
        let name = self.consume(&TokenType::Identifier, "Expected property name after '.'.")?;
        Ok(Expr::Get {
            object: Box::new(left),
            name,
        })
    }

    /// Parsea una expresión condicional ternaria (`condition ? then_branch : else_branch`).
    fn ternary(&mut self, condition: Expr) -> Result<Expr, String> {
        let then_branch = self.expression()?;
        self.consume(
            &TokenType::Colon,
            "Expected ':' after then branch of conditional expression.",
        )?;
        let else_branch = self.parse_precedence(Precedence::Ternary)?;
        Ok(Expr::Ternary {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
        })
    }
}
