//! Recursive-descent parser.
//!
//! The level nesting follows the fx-50FH II user's guide priority sequence
//! (1 = tightest):
//!
//! 1. parenthetical functions / atoms
//! 2. postfix (`²`, `³`, `⁻¹`, `!`, `%`) and `^(` / `x√(`
//! 3. fractions `┘`
//! 4. prefix negation
//! 5. statistical estimated values (not yet supported)
//! 6. `nPr`, `nCr`
//! 7. `×`, `÷` and omitted multiplication
//! 8. `+`, `-`
//! 9. relational operators
//! 10. `and`
//! 11. `or`, `xor`, `xnor`

use crate::ast::{Expr, MemOp, Setup, Stmt, UnaryOp};
use crate::bases::Base;
use crate::error::CalcError;
use crate::token::{BinOp, FuncName, Token, TokenKind};
use crate::value::ComplexFormat;

pub(crate) fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, CalcError> {
    Parser {
        tokens,
        current: 0,
        depth: 0,
    }
    .program()
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
    /// How many expression levels are currently open. See [`MAX_DEPTH`].
    depth: usize,
}

/// How deep expressions may nest before the machine reports `Stack ERROR`.
///
/// The hardware has a finite calculation stack and shows `Stack ERROR` when it
/// overflows. This interpreter had no bound at all, so deeply nested input did
/// not produce that screen — it overflowed the *host* stack and aborted the
/// process with `SIGABRT`. The two entry points that recurse, [`Parser::expression`]
/// and [`Parser::unary`], now count levels and report the machine's error
/// instead.
///
/// The number is a safety bound, **not** the hardware's figure: the machine's
/// exact stack depth is not recorded in the documentation this project has
/// available, and being permissive is the safer error — a limit set too low
/// would reject programs the real calculator runs, whereas one set too high
/// only accepts input the hardware would have refused. Tune it here when the
/// manual turns up.
const MAX_DEPTH: usize = 256;

impl Parser {
    // -- token helpers ------------------------------------------------------

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn check(&self, kind: &TokenKind) -> bool {
        &self.peek().kind == kind
    }

    fn check_var(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Var(_))
    }

    fn advance(&mut self) -> Token {
        if !self.at_eof() {
            self.current += 1;
        }
        self.previous().clone()
    }

    fn matches(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, what: &str) -> Result<Token, CalcError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error(format!("expected {what}")))
        }
    }

    fn error(&self, message: impl Into<String>) -> CalcError {
        CalcError::syntax(message, self.peek().pos)
    }

    fn skip_colons(&mut self) {
        while self.matches(&TokenKind::Colon) {}
    }

    /// Tokens that terminate an expression.
    fn at_boundary(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Colon
                | TokenKind::Display
                | TokenKind::Eof
                | TokenKind::Then
                | TokenKind::Else
                | TokenKind::IfEnd
                | TokenKind::WhileEnd
                | TokenKind::Next
                | TokenKind::Step
                | TokenKind::To
                | TokenKind::CondJump
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::DT
                | TokenKind::RParen
        )
    }

    // -- program / statements ----------------------------------------------

    fn program(&mut self) -> Result<Vec<Stmt>, CalcError> {
        let mut statements = Vec::new();
        self.skip_colons();
        while !self.at_eof() {
            statements.push(self.statement()?);
            self.skip_colons();
        }
        // The `#mode` directive configures the whole program, so it may only
        // appear once and only at the head.
        for (index, stmt) in statements.iter().enumerate() {
            if matches!(stmt, Stmt::Mode(_)) && index != 0 {
                return Err(CalcError::syntax_here(
                    "`#mode` must be the first statement in the program",
                ));
            }
        }
        Ok(statements)
    }

    fn statement(&mut self) -> Result<Stmt, CalcError> {
        if self.check(&TokenKind::Colon) {
            self.advance();
            return Ok(Stmt::Noop);
        }

        let base = self.simple_statement()?;

        if self.matches(&TokenKind::CondJump) {
            let target = self.jump_target()?;
            return Ok(Stmt::CondJump {
                condition: expr_of(base)?,
                target: Box::new(target),
            });
        }

        if self.matches(&TokenKind::Display) {
            return Ok(apply_display(base));
        }

        Ok(base)
    }

    /// The statement following `⇒`.
    fn jump_target(&mut self) -> Result<Stmt, CalcError> {
        if self.matches(&TokenKind::Display) {
            return Ok(Stmt::Expr {
                expr: Expr::Number(0.0),
                display: true,
            });
        }
        let base = self.simple_statement()?;
        if self.matches(&TokenKind::Display) {
            Ok(apply_display(base))
        } else {
            Ok(base)
        }
    }

    fn simple_statement(&mut self) -> Result<Stmt, CalcError> {
        // The mode directive is a statement so that the interpreter and the
        // checker can see it in the flattened program.
        if let TokenKind::ModeDirective(mode) = self.peek().kind {
            self.advance();
            return Ok(Stmt::Mode(mode));
        }
        match self.peek().kind.clone() {
            TokenKind::ClrMemory => {
                self.advance();
                return Ok(Stmt::ClrMemory);
            }
            TokenKind::ClrStat => {
                self.advance();
                return Ok(Stmt::ClrStat);
            }
            TokenKind::FreqOn => {
                self.advance();
                return Ok(Stmt::FreqOn);
            }
            TokenKind::FreqOff => {
                self.advance();
                return Ok(Stmt::FreqOff);
            }
            TokenKind::Dec => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Dec));
            }
            TokenKind::Hex => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Hex));
            }
            TokenKind::Bin => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Bin));
            }
            TokenKind::Oct => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Oct));
            }
            TokenKind::ComplexFormat(format) => {
                self.advance();
                return Ok(Stmt::Setup(match format {
                    ComplexFormat::Cartesian => Setup::ComplexCartesian,
                    ComplexFormat::Polar => Setup::ComplexPolar,
                }));
            }
            TokenKind::ReIm => {
                self.advance();
                return Ok(Stmt::Setup(Setup::ReIm));
            }
            TokenKind::Regression(reg) => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Reg(reg)));
            }
            TokenKind::DmsToggle => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Sexagesimal));
            }
            TokenKind::Deg => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Deg));
            }
            TokenKind::Rad => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Rad));
            }
            TokenKind::Gra => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Gra));
            }
            TokenKind::Fix => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Fix(self.setup_digit()?)));
            }
            TokenKind::Sci => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Sci(self.setup_digit()?)));
            }
            TokenKind::Norm => {
                self.advance();
                return Ok(Stmt::Setup(Setup::Norm(self.setup_digit()?)));
            }
            TokenKind::Then => {
                self.advance();
                return Ok(Stmt::Then);
            }
            TokenKind::Else => {
                self.advance();
                return Ok(Stmt::Else);
            }
            TokenKind::IfEnd => {
                self.advance();
                return Ok(Stmt::IfEnd);
            }
            TokenKind::WhileEnd => {
                self.advance();
                return Ok(Stmt::WhileEnd);
            }
            TokenKind::Next => {
                self.advance();
                return Ok(Stmt::Next);
            }
            TokenKind::Break => {
                self.advance();
                return Ok(Stmt::Break);
            }
            TokenKind::Lbl => {
                self.advance();
                let label = self.label_number()?;
                return Ok(Stmt::Label(label));
            }
            TokenKind::Goto => {
                self.advance();
                let label = self.label_number()?;
                return Ok(Stmt::Goto(label));
            }
            TokenKind::If => {
                self.advance();
                let condition = self.expression(false)?;
                return Ok(Stmt::If { condition });
            }
            TokenKind::While => {
                self.advance();
                let condition = self.expression(false)?;
                return Ok(Stmt::While { condition });
            }
            TokenKind::For => {
                self.advance();
                let init = self.expression(true)?;
                let (var, from) = match init {
                    Expr::Assign { target, value } => (target, *value),
                    _ => return Err(self.error("`For` expects an assignment like `For 1→A`")),
                };
                self.expect(&TokenKind::To, "`To` in `For`")?;
                let to = self.expression(false)?;
                let step = if self.matches(&TokenKind::Step) {
                    Some(self.expression(false)?)
                } else {
                    None
                };
                return Ok(Stmt::For {
                    var,
                    from,
                    to,
                    step,
                });
            }
            TokenKind::DT => {
                return Err(self.error("`DT` must follow a data value"));
            }
            _ => {}
        }

        // Expression, assignment, statistical data entry, or memory
        // arithmetic.
        let expression = self.expression(true)?;
        if self.check(&TokenKind::Comma)
            || self.check(&TokenKind::Semicolon)
            || self.check(&TokenKind::DT)
        {
            return self.data_entry(expression);
        }
        let statement = match expression {
            Expr::Assign { target, value } => Stmt::Assign {
                target,
                value: *value,
                display: false,
            },
            other => Stmt::Expr {
                expr: other,
                display: false,
            },
        };

        if self.matches(&TokenKind::MPlus) {
            return match statement {
                Stmt::Expr { expr, .. } => Ok(Stmt::Memory {
                    expr,
                    op: MemOp::Plus,
                    display: false,
                }),
                _ => Err(self.error("`M+` must follow an expression")),
            };
        }
        if self.matches(&TokenKind::MMinus) {
            return match statement {
                Stmt::Expr { expr, .. } => Ok(Stmt::Memory {
                    expr,
                    op: MemOp::Minus,
                    display: false,
                }),
                _ => Err(self.error("`M-` must follow an expression")),
            };
        }

        Ok(statement)
    }

    /// Parse the tail of a `DT` data-entry statement once the first value has
    /// been read: `DT`, `,y DT`, `;f DT` or `,y;f DT`.
    fn data_entry(&mut self, x: Expr) -> Result<Stmt, CalcError> {
        let y = if self.matches(&TokenKind::Comma) {
            Some(self.expression(false)?)
        } else {
            None
        };
        let freq = if self.matches(&TokenKind::Semicolon) {
            Some(self.expression(false)?)
        } else {
            None
        };
        self.expect(&TokenKind::DT, "`DT`")?;
        Ok(Stmt::DataEntry { x, y, freq })
    }

    fn setup_digit(&mut self) -> Result<u8, CalcError> {
        let token = self.advance();
        match token.kind {
            TokenKind::Number(ref s) => s
                .parse::<u8>()
                .ok()
                .filter(|d| *d <= 9)
                .ok_or_else(|| CalcError::syntax("expected a digit 0-9", token.pos)),
            _ => Err(CalcError::syntax("expected a digit 0-9", token.pos)),
        }
    }

    fn label_number(&mut self) -> Result<u8, CalcError> {
        let token = self.advance();
        match token.kind {
            TokenKind::Number(ref s) if s.len() == 1 => s
                .parse::<u8>()
                .ok()
                .filter(|d| *d <= 9)
                .ok_or_else(|| CalcError::syntax("label must be a single digit 0-9", token.pos)),
            _ => Err(CalcError::syntax(
                "label must be a single digit 0-9",
                token.pos,
            )),
        }
    }

    // -- expressions --------------------------------------------------------

    fn expression(&mut self, allow_assignment: bool) -> Result<Expr, CalcError> {
        self.enter()?;
        let result = self.expression_inner(allow_assignment);
        self.depth -= 1;
        result
    }

    /// Count one open expression level, or report the machine's `Stack ERROR`.
    fn enter(&mut self) -> Result<(), CalcError> {
        if self.depth >= MAX_DEPTH {
            return Err(CalcError::Stack);
        }
        self.depth += 1;
        Ok(())
    }

    fn expression_inner(&mut self, allow_assignment: bool) -> Result<Expr, CalcError> {
        let expr = self.or()?;
        if allow_assignment && self.matches(&TokenKind::Assign) {
            if !self.check_var() {
                return Err(self.error("expected a variable after `→`"));
            }
            let target = match self.advance().kind {
                TokenKind::Var(v) => v,
                _ => unreachable!(),
            };
            return Ok(Expr::Assign {
                target,
                value: Box::new(expr),
            });
        }
        Ok(expr)
    }

    fn binary_level(
        &mut self,
        ops: &[BinOp],
        next: fn(&mut Self) -> Result<Expr, CalcError>,
    ) -> Result<Expr, CalcError> {
        let mut left = next(self)?;
        loop {
            if self.at_boundary() {
                return Ok(left);
            }
            match &self.peek().kind {
                TokenKind::Op(op) if ops.contains(op) => {
                    let op = *op;
                    self.advance();
                    let right = next(self)?;
                    left = Expr::Binary {
                        left: Box::new(left),
                        op,
                        right: Box::new(right),
                    };
                }
                _ => return Ok(left),
            }
        }
    }

    fn or(&mut self) -> Result<Expr, CalcError> {
        self.binary_level(&[BinOp::Or, BinOp::Xor, BinOp::Xnor], Self::and)
    }

    fn and(&mut self) -> Result<Expr, CalcError> {
        self.binary_level(&[BinOp::And], Self::relational)
    }

    fn relational(&mut self) -> Result<Expr, CalcError> {
        self.binary_level(
            &[
                BinOp::Eq,
                BinOp::Ne,
                BinOp::Gt,
                BinOp::Lt,
                BinOp::Ge,
                BinOp::Le,
            ],
            Self::additive,
        )
    }

    fn additive(&mut self) -> Result<Expr, CalcError> {
        self.binary_level(&[BinOp::Add, BinOp::Sub], Self::polar)
    }

    /// `r∠θ`, tighter than `+` but looser than `×`.
    fn polar(&mut self) -> Result<Expr, CalcError> {
        self.binary_level(&[BinOp::Polar], Self::multiplicative)
    }

    fn multiplicative(&mut self) -> Result<Expr, CalcError> {
        let mut left = self.permutation()?;
        loop {
            if self.at_boundary() {
                return Ok(left);
            }
            if let TokenKind::Op(op @ (BinOp::Mul | BinOp::Div)) = self.peek().kind {
                self.advance();
                let right = self.permutation()?;
                left = Expr::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                };
            } else if self.starts_implicit_factor() {
                let right = self.permutation()?;
                left = Expr::ImplicitMul(Box::new(left), Box::new(right));
            } else {
                return Ok(left);
            }
        }
    }

    fn starts_implicit_factor(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Var(_)
                | TokenKind::Const(_)
                | TokenKind::Func(_)
                | TokenKind::LParen
                | TokenKind::Ran
                | TokenKind::Input
                | TokenKind::StatVar(_)
        )
    }

    fn permutation(&mut self) -> Result<Expr, CalcError> {
        self.binary_level(&[BinOp::Perm, BinOp::Comb], Self::unary)
    }

    fn unary(&mut self) -> Result<Expr, CalcError> {
        // `-(-(-…))` recurses through this function without passing through
        // `expression`, so it needs counting too.
        self.enter()?;
        let result = self.unary_inner();
        self.depth -= 1;
        result
    }

    fn unary_inner(&mut self) -> Result<Expr, CalcError> {
        if let TokenKind::Op(BinOp::Sub) = self.peek().kind {
            self.advance();
            let expr = self.unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            });
        }
        self.fraction()
    }

    fn fraction(&mut self) -> Result<Expr, CalcError> {
        let mut left = self.power()?;
        while !self.at_boundary() && self.check(&TokenKind::Op(BinOp::Frac)) {
            self.advance();
            let right = self.power()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinOp::Frac,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn power(&mut self) -> Result<Expr, CalcError> {
        let mut value = self.atom()?;
        loop {
            if self.at_boundary() {
                return Ok(value);
            }
            match self.peek().kind.clone() {
                TokenKind::Postfix(p) => {
                    self.advance();
                    value = Expr::Unary {
                        op: match p {
                            crate::token::Postfix::Inverse => UnaryOp::Inverse,
                            crate::token::Postfix::Square => UnaryOp::Square,
                            crate::token::Postfix::Cube => UnaryOp::Cube,
                            crate::token::Postfix::Fact => UnaryOp::Fact,
                            crate::token::Postfix::Percent => UnaryOp::Percent,
                        },
                        expr: Box::new(value),
                    };
                }
                TokenKind::Pow => {
                    self.advance();
                    let exp = self.grouped_argument()?;
                    value = Expr::Pow {
                        base: Box::new(value),
                        exp: Box::new(exp),
                    };
                }
                TokenKind::Root => {
                    self.advance();
                    let radicand = self.grouped_argument()?;
                    value = Expr::Root {
                        index: Box::new(value),
                        radicand: Box::new(radicand),
                    };
                }
                _ => return Ok(value),
            }
        }
    }

    /// One argument of `^(`, `x√(`, optionally wrapped in parentheses.
    fn grouped_argument(&mut self) -> Result<Expr, CalcError> {
        if self.matches(&TokenKind::LParen) {
            let expr = self.expression(false)?;
            self.matches(&TokenKind::RParen);
            Ok(expr)
        } else {
            self.unary()
        }
    }

    fn atom(&mut self) -> Result<Expr, CalcError> {
        match self.peek().kind.clone() {
            TokenKind::Number(text) => {
                self.advance();
                let value = parse_number(&text).ok_or_else(|| {
                    CalcError::syntax(format!("invalid number `{text}`"), self.previous().pos)
                })?;
                // Remember a base tag (`1Fh`) so the mode checker can require
                // BASE mode for it.
                match Base::tag_of(&text) {
                    Some(base) => Ok(Expr::BaseLiteral { value, base }),
                    None => Ok(Expr::Number(value)),
                }
            }
            TokenKind::Sexagesimal(value) => {
                let pos = self.peek().pos;
                self.advance();
                Ok(Expr::Sexagesimal(value, pos))
            }
            TokenKind::Var(v) => {
                self.advance();
                Ok(Expr::Var(v))
            }
            TokenKind::Const(c) => {
                self.advance();
                Ok(Expr::Const(c))
            }
            TokenKind::Ran => {
                self.advance();
                Ok(Expr::Ran)
            }
            TokenKind::StatVar(var) => {
                self.advance();
                Ok(Expr::StatVar(var))
            }
            TokenKind::Input => {
                self.advance();
                Ok(Expr::Input)
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.expression(false)?;
                self.matches(&TokenKind::RParen);
                Ok(expr)
            }
            TokenKind::Func(func) => {
                self.advance();
                self.function_call(func)
            }
            _ => Err(self.error(format!(
                "expected an expression, found {}",
                self.peek().kind.describe()
            ))),
        }
    }

    fn function_call(&mut self, func: FuncName) -> Result<Expr, CalcError> {
        let mut args = Vec::new();
        if self.matches(&TokenKind::LParen) {
            if !self.check(&TokenKind::RParen) {
                loop {
                    args.push(self.expression(false)?);
                    if !self.matches(&TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.matches(&TokenKind::RParen);
        } else {
            // The opening parenthesis may have been omitted at the end of input.
            args.push(self.unary()?);
        }

        let (min, max) = arity(func);
        if args.len() < min || args.len() > max {
            return Err(self.error(format!(
                "`{}` expects {}{} argument(s), got {}",
                func_name(func),
                min,
                if max > min {
                    format!(" to {max}")
                } else {
                    String::new()
                },
                args.len()
            )));
        }
        Ok(Expr::Call { func, args })
    }
}

fn expr_of(stmt: Stmt) -> Result<Expr, CalcError> {
    match stmt {
        Stmt::Expr { expr, .. } => Ok(expr),
        _ => Err(CalcError::syntax_here(
            "`⇒` must be preceded by an expression",
        )),
    }
}

fn apply_display(stmt: Stmt) -> Stmt {
    match stmt {
        Stmt::Expr { expr, .. } => Stmt::Expr {
            expr,
            display: true,
        },
        Stmt::Assign { target, value, .. } => Stmt::Assign {
            target,
            value,
            display: true,
        },
        Stmt::Memory { expr, op, .. } => Stmt::Memory {
            expr,
            op,
            display: true,
        },
        other => other,
    }
}

fn arity(func: FuncName) -> (usize, usize) {
    match func {
        FuncName::Log => (1, 2),
        FuncName::Pol | FuncName::Rec => (2, 2),
        _ => (1, 1),
    }
}

fn func_name(func: FuncName) -> &'static str {
    use FuncName::*;
    match func {
        Sin => "sin",
        Cos => "cos",
        Tan => "tan",
        Asin => "sin⁻¹",
        Acos => "cos⁻¹",
        Atan => "tan⁻¹",
        Sinh => "sinh",
        Cosh => "cosh",
        Tanh => "tanh",
        Asinh => "sinh⁻¹",
        Acosh => "cosh⁻¹",
        Atanh => "tanh⁻¹",
        Log => "log",
        Ln => "ln",
        Sqrt => "√",
        Cbrt => "∛",
        TenPow => "10^",
        EPow => "e^",
        Abs => "Abs",
        Pol => "Pol",
        Rec => "Rec",
        Rnd => "Rnd",
        Arg => "arg",
        Conjg => "Conjg",
        Not => "Not",
        Neg => "Neg",
    }
}

fn parse_number(text: &str) -> Option<f64> {
    // Base-tagged literals such as `1Fh`, `1010b`, `17o`, `42d`.
    if let Some(value) = crate::bases::Base::parse(text) {
        return Some(value);
    }
    if let Ok(v) = text.parse::<f64>() {
        return Some(v);
    }
    // Accept a bare trailing/leading dot and Casio-style `E` exponents.
    let normalized = text.replace('E', "e");
    normalized.parse::<f64>().ok()
}
