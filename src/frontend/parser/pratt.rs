use super::Parser;
use crate::expr::{Expr, LiteralValue};
use crate::token::{Literal, TokenType};
 
/// De menor a mayor. El orden de las variantes ES la precedencia.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precedence {
    Lowest,
    Or,
    And,
    Equality,
    Comparison,
    Term,
    Factor,
    Unary,
    Call,
}
 
impl Precedence {
    fn next(self) -> Self {
        use Precedence::*;
        match self {
            Lowest => Or,
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
 
struct ParseRule {
    prefix: Option<PrefixFn>,
    infix: Option<InfixFn>,
    precedence: Precedence,
}
 
fn rule(prefix: Option<PrefixFn>, infix: Option<InfixFn>, precedence: Precedence) -> ParseRule {
    ParseRule {
        prefix,
        infix,
        precedence,
    }
}
 
#[rustfmt::skip]
fn get_rule(t: &TokenType) -> ParseRule {
    use Precedence as P;
    use TokenType as T;
    //                                    prefix                      infix                    precedence
    match t {
        T::LeftParen                  => rule(Some(Parser::grouping), Some(Parser::finish_call), P::Call),
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
 
    // ---- prefix: el token ya fue consumido y está en previous() ----
 
    fn grouping(&mut self) -> Result<Expr, String> {
        let expr = self.expression()?;
        self.consume(&TokenType::RightParen, "Expected ')' after expression.")?;
        Ok(Expr::Grouping {
            expression: Box::new(expr),
        })
    }
 
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
 
    fn literal(&mut self) -> Result<Expr, String> {
        let value = match self.previous().token_type {
            TokenType::True => LiteralValue::Bool(true),
            TokenType::False => LiteralValue::Bool(false),
            _ => LiteralValue::Nil,
        };
        Ok(Expr::Literal { value })
    }
 
    fn variable(&mut self) -> Result<Expr, String> {
        Ok(Expr::Variable {
            name: self.previous().clone(),
        })
    }
 
    fn this_expr(&mut self) -> Result<Expr, String> {
        Ok(Expr::This {
            keyword: self.previous().clone(),
        })
    }
 
    fn super_expr(&mut self) -> Result<Expr, String> {
        let keyword = self.previous().clone();
        self.consume(&TokenType::Dot, "Expected '.' after 'super'.")?;
        let method = self.consume(&TokenType::Identifier, "Expected superclass method name.")?;
        Ok(Expr::Super { keyword, method })
    }
 
    fn unary(&mut self) -> Result<Expr, String> {
        let operator = self.previous().clone();
        let right = self.parse_precedence(Precedence::Unary)?;
        Ok(Expr::Unary {
            operator,
            right: Box::new(right),
        })
    }
 
    // ---- infix: el operador está en previous(), el lado izquierdo ya está parseado ----
 
    fn binary(&mut self, left: Expr) -> Result<Expr, String> {
        let operator = self.previous().clone();
        let prec = get_rule(&operator.token_type).precedence;
        let right = self.parse_precedence(prec.next())?; // next() => asociatividad a izquierda
        Ok(Expr::Binary {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        })
    }
 
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
 
    fn dot(&mut self, left: Expr) -> Result<Expr, String> {
        let name = self.consume(&TokenType::Identifier, "Expected property name after '.'.")?;
        Ok(Expr::Get {
            object: Box::new(left),
            name,
        })
    }
}
 