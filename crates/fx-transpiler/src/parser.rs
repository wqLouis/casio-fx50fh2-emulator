//! Recursive-descent parser for the C-like `.fxc` language.
//!
//! Grammar (informal):
//!
//! ```text
//! program   := stmt*
//! stmt      := 'let' NAME '=' expr ';'
//!            | 'const' NAME '=' expr ';'
//!            | NAME '=' expr ';'
//!            | 'print' '(' expr ')' ';'
//!            | 'if' '(' expr ')' block ('else' block)?
//!            | 'while' '(' expr ')' block
//!            | 'for' '(' for_init ';' expr ';' NAME '=' expr ')' block
//!            | 'break' ';'
//!            | 'goto' NUMBER ';'
//!            | 'label' NUMBER ';'
//!            | '{' stmt* '}'
//!            | ';'
//!            | expr ';'
//! expr      := equality
//! equality  := comparison (('==' | '!=') comparison)*
//! comparison:= additive (('<' | '<=' | '>' | '>=') additive)*
//! additive  := multiplicative (('+' | '-') multiplicative)*
//! multiplicative := unary (('*' | '/') unary)*
//! unary     := '-' unary | power
//! power     := primary (('^' | '**') unary)?
//! primary   := NUMBER | NAME | NAME '(' args ')' | 'pi' | 'e' | 'input()'
//!            | 'phys' '.' NAME | NAME accessor+ | '(' expr ')'
//! accessor  := '.' NAME | '[' NUMBER ']'
//! ```

use crate::ast::{Accessor, BinOp, Expr, ForStmt, Program, Stmt, UnOp};
use crate::builtins;
use crate::error::TranspileError;
use crate::lexer::{Tok, Token};

/// Parse a token stream (as produced by [`crate::lexer::lex`]).
pub fn parse(tokens: &[Token], source: &str) -> Result<Program, TranspileError> {
    Parser {
        tokens,
        source,
        current: 0,
    }
    .program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    source: &'a str,
    current: usize,
}

impl<'a> Parser<'a> {
    // -- token helpers ------------------------------------------------------

    fn peek(&self) -> &Tok {
        &self.tokens[self.current].tok
    }

    fn peek_at(&self, offset: usize) -> &Tok {
        let index = (self.current + offset).min(self.tokens.len() - 1);
        &self.tokens[index].tok
    }

    fn position(&self) -> usize {
        self.tokens[self.current].pos
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), Tok::Eof)
    }

    fn check(&self, tok: &Tok) -> bool {
        self.peek() == tok
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.current].clone();
        if !self.at_eof() {
            self.current += 1;
        }
        token
    }

    fn matches(&mut self, tok: &Tok) -> bool {
        if self.check(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: &Tok, what: &str) -> Result<Token, TranspileError> {
        if self.check(tok) {
            Ok(self.advance())
        } else {
            Err(self.error(format!("expected {what}, found {}", self.peek().describe())))
        }
    }

    fn error(&self, message: impl Into<String>) -> TranspileError {
        TranspileError::at(self.source, message, self.position())
    }

    fn error_at(&self, message: impl Into<String>, offset: usize) -> TranspileError {
        TranspileError::at(self.source, message, offset)
    }

    /// Consume an `Ident` or fail.
    fn expect_ident(&mut self, what: &str) -> Result<(String, usize), TranspileError> {
        let pos = self.position();
        match self.peek().clone() {
            Tok::Ident(name) => {
                self.advance();
                Ok((name, pos))
            }
            other => {
                Err(self.error_at(format!("expected {what}, found {}", other.describe()), pos))
            }
        }
    }

    // -- program / statements ----------------------------------------------

    fn program(&mut self) -> Result<Program, TranspileError> {
        // Directives (`#mode`, `#reg`) configure the whole program. The
        // emitter reads them from the token stream, so the parser drops the
        // whole leading run here.
        while matches!(self.peek(), Tok::Mode(_) | Tok::Reg(..)) {
            self.advance();
        }
        let mut statements = Vec::new();
        while !self.at_eof() {
            statements.push(self.statement()?);
        }
        Ok(statements)
    }

    fn statement(&mut self) -> Result<Stmt, TranspileError> {
        match self.peek().clone() {
            Tok::Let => self.let_statement(),
            Tok::Const => self.const_statement(),
            Tok::Print => self.print_statement(),
            Tok::If => self.if_statement(),
            Tok::While => self.while_statement(),
            Tok::For => self.for_statement(),
            Tok::Break => {
                self.advance();
                self.expect(&Tok::Semi, "`;` after `break`")?;
                Ok(Stmt::Break)
            }
            Tok::Goto => {
                let pos = self.position();
                self.advance();
                let label = self.label_number("`goto`")?;
                self.expect(&Tok::Semi, "`;` after `goto`")?;
                Ok(Stmt::Goto(label, pos))
            }
            Tok::Label => {
                let pos = self.position();
                self.advance();
                let label = self.label_number("`label`")?;
                self.expect(&Tok::Semi, "`;` after `label`")?;
                Ok(Stmt::Label(label, pos))
            }
            Tok::LBrace => self.block(),
            Tok::Semi => {
                self.advance();
                Ok(Stmt::Empty)
            }
            _ => self.assign_or_expression(),
        }
    }

    fn label_number(&mut self, what: &str) -> Result<u8, TranspileError> {
        let pos = self.position();
        match self.peek().clone() {
            Tok::Number(value) if value.fract() == 0.0 && (0.0..=9.0).contains(&value) => {
                self.advance();
                Ok(value as u8)
            }
            _ => Err(self.error_at(format!("{what} label must be a single digit 0-9"), pos)),
        }
    }

    fn let_statement(&mut self) -> Result<Stmt, TranspileError> {
        self.advance();
        let (name, pos) = self.expect_ident("a variable name after `let`")?;
        self.expect(&Tok::Assign, "`=` in `let`")?;
        let value = self.expression()?;
        self.expect(&Tok::Semi, "`;` after `let`")?;
        Ok(Stmt::Let { name, value, pos })
    }

    fn const_statement(&mut self) -> Result<Stmt, TranspileError> {
        self.advance();
        let (name, pos) = self.expect_ident("a name after `const`")?;
        self.expect(&Tok::Assign, "`=` in `const`")?;
        let value = self.expression()?;
        self.expect(&Tok::Semi, "`;` after `const`")?;
        Ok(Stmt::Const { name, value, pos })
    }

    fn assign_or_expression(&mut self) -> Result<Stmt, TranspileError> {
        // `NAME = expr;` is an assignment; anything else is an expression.
        if let (Tok::Ident(name), Tok::Assign) = (self.peek().clone(), self.peek_at(1).clone()) {
            let pos = self.position();
            self.advance();
            self.advance();
            let value = self.expression()?;
            self.expect(&Tok::Semi, "`;` after assignment")?;
            return Ok(Stmt::Assign { name, value, pos });
        }
        let value = self.expression()?;
        self.expect(&Tok::Semi, "`;` after expression")?;
        Ok(Stmt::ExprStmt(value))
    }

    fn print_statement(&mut self) -> Result<Stmt, TranspileError> {
        self.advance();
        let value = if self.matches(&Tok::LParen) {
            let value = self.expression()?;
            self.expect(&Tok::RParen, "`)` after `print(`")?;
            value
        } else {
            self.expression()?
        };
        self.expect(&Tok::Semi, "`;` after `print`")?;
        Ok(Stmt::Print(value))
    }

    fn if_statement(&mut self) -> Result<Stmt, TranspileError> {
        self.advance();
        self.expect(&Tok::LParen, "`(` after `if`")?;
        let cond = self.expression()?;
        self.expect(&Tok::RParen, "`)` after the `if` condition")?;
        let then_body = self.body()?;
        let else_body = if self.matches(&Tok::Else) {
            self.body()?
        } else {
            Vec::new()
        };
        Ok(Stmt::If {
            cond,
            then_body,
            else_body,
        })
    }

    fn while_statement(&mut self) -> Result<Stmt, TranspileError> {
        self.advance();
        self.expect(&Tok::LParen, "`(` after `while`")?;
        let cond = self.expression()?;
        self.expect(&Tok::RParen, "`)` after the `while` condition")?;
        let body = self.body()?;
        Ok(Stmt::While { cond, body })
    }

    fn for_statement(&mut self) -> Result<Stmt, TranspileError> {
        let pos = self.position();
        self.advance();
        self.expect(&Tok::LParen, "`(` after `for`")?;

        let (init_name, init_value, is_decl) = if self.matches(&Tok::Let) {
            let (name, _) = self.expect_ident("a variable name after `let`")?;
            self.expect(&Tok::Assign, "`=` in the `for` initialiser")?;
            (name, self.expression()?, true)
        } else {
            let (name, _) = self.expect_ident("a variable name in the `for` initialiser")?;
            self.expect(&Tok::Assign, "`=` in the `for` initialiser")?;
            (name, self.expression()?, false)
        };

        self.expect(&Tok::Semi, "`;` after the `for` initialiser")?;
        let cond = self.expression()?;
        self.expect(&Tok::Semi, "`;` after the `for` condition")?;
        let (update_name, _) = self.expect_ident("a variable name in the `for` update")?;
        self.expect(&Tok::Assign, "`=` in the `for` update")?;
        let update_value = self.expression()?;
        self.expect(&Tok::RParen, "`)` after the `for` clauses")?;

        let body = self.body()?;
        Ok(Stmt::For(ForStmt {
            init_name,
            init_value,
            is_decl,
            cond,
            update_name,
            update_value,
            body,
            pos,
        }))
    }

    fn body(&mut self) -> Result<Vec<Stmt>, TranspileError> {
        if self.check(&Tok::LBrace) {
            match self.block()? {
                Stmt::Block(stmts) => Ok(stmts),
                other => Ok(vec![other]),
            }
        } else {
            Ok(vec![self.statement()?])
        }
    }

    fn block(&mut self) -> Result<Stmt, TranspileError> {
        self.expect(&Tok::LBrace, "`{`")?;
        let mut statements = Vec::new();
        while !self.check(&Tok::RBrace) && !self.at_eof() {
            statements.push(self.statement()?);
        }
        self.expect(&Tok::RBrace, "`}`")?;
        Ok(Stmt::Block(statements))
    }

    // -- expressions --------------------------------------------------------

    fn expression(&mut self) -> Result<Expr, TranspileError> {
        self.equality()
    }

    fn equality(&mut self) -> Result<Expr, TranspileError> {
        let mut left = self.comparison()?;
        loop {
            let op = match self.peek() {
                Tok::Eq => BinOp::Eq,
                Tok::Ne => BinOp::Ne,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.comparison()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn comparison(&mut self) -> Result<Expr, TranspileError> {
        let mut left = self.additive()?;
        loop {
            let op = match self.peek() {
                Tok::Lt => BinOp::Lt,
                Tok::Le => BinOp::Le,
                Tok::Gt => BinOp::Gt,
                Tok::Ge => BinOp::Ge,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.additive()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn additive(&mut self) -> Result<Expr, TranspileError> {
        let mut left = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.multiplicative()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, TranspileError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.unary()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Result<Expr, TranspileError> {
        if self.matches(&Tok::Minus) {
            let operand = self.unary()?;
            return Ok(Expr::Unary(UnOp::Neg, Box::new(operand)));
        }
        self.power()
    }

    fn power(&mut self) -> Result<Expr, TranspileError> {
        let base = self.primary()?;
        if self.matches(&Tok::Caret) || self.matches(&Tok::Power) {
            // Right-associative, and the exponent may itself be unary (`2^-1`).
            let exp = self.unary()?;
            return Ok(Expr::Binary(BinOp::Pow, Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    fn primary(&mut self) -> Result<Expr, TranspileError> {
        let pos = self.position();
        match self.peek().clone() {
            Tok::Number(value) => {
                self.advance();
                Ok(Expr::Number(value))
            }
            Tok::LParen => {
                self.advance();
                let expr = self.expression()?;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(expr)
            }
            Tok::Ident(name) => {
                self.advance();
                if self.check(&Tok::LParen) {
                    self.call(name, pos)
                } else if matches!(self.peek(), Tok::Dot | Tok::LBracket) {
                    let accessors = self.accessors()?;
                    Ok(Expr::Data {
                        name,
                        accessors,
                        pos,
                    })
                } else {
                    match name.as_str() {
                        "pi" => Ok(Expr::Pi(pos)),
                        "e" => Ok(Expr::E(pos)),
                        _ => Ok(Expr::Name(name, pos)),
                    }
                }
            }
            Tok::Phys => self.constant(pos),
            other => Err(self.error_at(
                format!("expected an expression, found {}", other.describe()),
                pos,
            )),
        }
    }

    /// Resolve `phys.NAME` to a scientific constant.
    ///
    /// The `phys` token has not been consumed yet. `NAME` may be the constant's
    /// ASCII name (`h`, `hbar`, `C0`) or its display symbol (`ħ`, `R∞`).
    fn constant(&mut self, pos: usize) -> Result<Expr, TranspileError> {
        self.expect(&Tok::Phys, "`phys`")?;
        if !self.matches(&Tok::Dot) {
            return Err(self.error_at(
                "`phys` must be followed by `.` and a constant name, as in `phys.h`",
                pos,
            ));
        }
        let (name, name_pos) = self.expect_ident("a constant name after `phys.`")?;
        let Some(constant) = crate::constants::lookup(&name) else {
            return Err(self.error_at(format!("unknown scientific constant `{name}`"), name_pos));
        };
        Ok(Expr::Constant(constant, pos))
    }

    /// Parse one or more accessors: `.field` or `[index]`.
    fn accessors(&mut self) -> Result<Vec<Accessor>, TranspileError> {
        let mut accessors = Vec::new();
        loop {
            if self.matches(&Tok::Dot) {
                let (name, pos) = self.expect_ident("a field name after `.`")?;
                accessors.push(Accessor::Field { name, pos });
            } else if self.matches(&Tok::LBracket) {
                let pos = self.position();
                let index = self.bracket_index()?;
                self.expect(&Tok::RBracket, "`]` after the index")?;
                accessors.push(Accessor::Index { index, pos });
            } else {
                return Ok(accessors);
            }
        }
    }

    /// A non-negative integer index inside `[ ]`.
    fn bracket_index(&mut self) -> Result<usize, TranspileError> {
        let pos = self.position();
        match self.peek().clone() {
            Tok::Number(value)
                if value.fract() == 0.0 && value >= 0.0 && value <= usize::MAX as f64 =>
            {
                self.advance();
                Ok(value as usize)
            }
            _ => Err(self.error_at("an array index must be a non-negative whole number", pos)),
        }
    }

    fn call(&mut self, name: String, pos: usize) -> Result<Expr, TranspileError> {
        self.expect(&Tok::LParen, "`(`")?;
        let mut args = Vec::new();
        if !self.check(&Tok::RParen) {
            loop {
                args.push(self.expression()?);
                if !self.matches(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(&Tok::RParen, "`)` after the arguments")?;

        if name == "input" {
            if !args.is_empty() {
                return Err(self.error_at("`input()` takes no arguments", pos));
            }
            return Ok(Expr::Input(pos));
        }

        let Some(builtin) = builtins::lookup(&name) else {
            return Err(self.error_at(format!("unknown function `{name}`"), pos));
        };
        if args.len() < builtin.min_args || args.len() > builtin.max_args {
            let expected = if builtin.min_args == builtin.max_args {
                format!("{}", builtin.min_args)
            } else {
                format!("{} to {}", builtin.min_args, builtin.max_args)
            };
            return Err(self.error_at(
                format!(
                    "`{}` expects {expected} argument(s), got {}",
                    name,
                    args.len()
                ),
                pos,
            ));
        }
        Ok(Expr::Call(name, args, pos))
    }
}
