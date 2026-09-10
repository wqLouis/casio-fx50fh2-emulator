//! Lexer for fx-50FH II programs.
//!
//! Source may be written with the calculator's own glyphs (`→`, `◢`, `⇒`, `┘`)
//! or with readable ASCII aliases (`->`, `disp`, `=>`, `/`).  Newlines are
//! treated exactly like the `:` statement separator, so a program can be laid
//! out one statement per line.

use crate::bases::Base;
use crate::error::CalcError;
use crate::mode::Mode;
use crate::stats::StatVar as SV;
use crate::token::{BinOp, ConstName, FuncName, Postfix, Token, TokenKind, VarName};
use crate::value::ComplexFormat;

/// Turn program source into a flat token stream terminated by [`TokenKind::Eof`].
pub fn lex(source: &str) -> Result<Vec<Token>, CalcError> {
    Lexer::new(source).run()
}

struct Lexer {
    chars: Vec<char>,
    /// Index into `chars`.
    i: usize,
    /// Byte offset of `chars[i]`.
    byte: usize,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Lexer {
            chars: source.chars().collect(),
            i: 0,
            byte: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.i + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += 1;
        self.byte += c.len_utf8();
        Some(c)
    }

    /// Does the remaining input start with `s`?
    fn looking_at(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(k, c)| self.peek_at(k) == Some(c))
    }

    fn consume_str(&mut self, s: &str) {
        for _ in s.chars() {
            self.advance();
        }
    }

    fn run(mut self) -> Result<Vec<Token>, CalcError> {
        let mut tokens = Vec::new();
        while let Some(c) = self.peek() {
            let pos = self.byte;
            let start = self.i;
            if c == '\n' || c == '\r' {
                self.advance();
                let lexeme: String = self.chars[start..self.i].iter().collect();
                tokens.push(Token {
                    kind: TokenKind::Colon,
                    lexeme,
                    pos,
                });
                continue;
            }
            if c.is_whitespace() {
                self.advance();
                continue;
            }
            // Line comment.
            if c == '/' && self.looking_at("//") {
                while let Some(ch) = self.peek() {
                    if ch == '\n' {
                        break;
                    }
                    self.advance();
                }
                continue;
            }
            if let Some(kind) = self.scan_token()? {
                let lexeme: String = self.chars[start..self.i].iter().collect();
                tokens.push(Token { kind, lexeme, pos });
            }
        }
        tokens.push(Token {
            kind: TokenKind::Eof,
            lexeme: String::new(),
            pos: self.byte,
        });
        Ok(tokens)
    }

    fn scan_token(&mut self) -> Result<Option<TokenKind>, CalcError> {
        let start = self.i;
        let c = self.peek().unwrap();

        // The `10^` key is the only keyword beginning with a digit.
        if self.looking_at("10^") {
            self.consume_str("10^");
            return Ok(Some(TokenKind::Func(FuncName::TenPow)));
        }

        // Complex-format setup keys (`▶a+b𝑖`, `▶r∠θ`, `>a+bi`, `>rangle`).
        for (text, format) in [
            ("▶a+b𝑖", ComplexFormat::Cartesian),
            ("▶a+b𝒾", ComplexFormat::Cartesian),
            ("▶a+bi", ComplexFormat::Cartesian),
            ("▶r∠θ", ComplexFormat::Polar),
            (">a+bi", ComplexFormat::Cartesian),
            (">rangle", ComplexFormat::Polar),
        ] {
            if self.looking_at(text) {
                self.consume_str(text);
                return Ok(Some(TokenKind::ComplexFormat(format)));
            }
        }

        // The leading `#mode NAME` directive.
        if c == '#' {
            if let Some(kind) = self.try_mode_directive()? {
                return Ok(Some(kind));
            }
            return Err(CalcError::syntax(
                "expected a `#mode COMP|CMPLX|BASE|SD|REG` directive",
                self.byte,
            ));
        }

        // Base-tagged integer literals (`1Fh`, `1010b`, `17o`, `42d`). This is
        // tried before identifier lexing so that a leading hex letter works
        // (`FFh` = 255). `Base::parse` rejects look-alikes — `Ab` is not
        // binary 0xA, so it still lexes as `A × B`, and `Abs`/`Deg`/`cos`
        // still lex as keywords.
        if c.is_ascii_hexdigit()
            && let Some(kind) = self.try_base_literal()
        {
            return Ok(Some(kind));
        }

        if c.is_alphabetic() {
            return self.scan_word(start);
        }

        if c.is_ascii_digit() || (c == '.' && self.peek_at(1).is_some_and(|d| d.is_ascii_digit())) {
            self.scan_number();
            let lexeme: String = self.chars[start..self.i].iter().collect();
            return Ok(Some(TokenKind::Number(lexeme)));
        }

        // Postfix power keys. Note that `^` is deliberately NOT folded
        // together with a following `(`: the `^(...)` power operator is
        // lexed as `Pow` + `LParen`, which is what the parser's
        // `grouped_argument` expects. Folding them made `2^(3+1)` fail.
        for (text, kind) in [
            ("^-1", TokenKind::Postfix(Postfix::Inverse)),
            ("⁻¹", TokenKind::Postfix(Postfix::Inverse)),
            ("^2", TokenKind::Postfix(Postfix::Square)),
            ("^3", TokenKind::Postfix(Postfix::Cube)),
        ] {
            if self.looking_at(text) {
                self.consume_str(text);
                return Ok(Some(kind));
            }
        }

        // Multi-character symbolic tokens, longest first.
        for (text, kind) in [
            ("->", TokenKind::Assign),
            ("=>", TokenKind::CondJump),
            ("<=", TokenKind::Op(BinOp::Le)),
            (">=", TokenKind::Op(BinOp::Ge)),
            ("<>", TokenKind::Op(BinOp::Ne)),
        ] {
            if self.looking_at(text) {
                self.consume_str(text);
                return Ok(Some(kind));
            }
        }

        let single = match c {
            '→' => TokenKind::Assign,
            '⇒' => TokenKind::CondJump,
            '◢' => TokenKind::Display,
            '≠' => TokenKind::Op(BinOp::Ne),
            '≥' => TokenKind::Op(BinOp::Ge),
            '≤' => TokenKind::Op(BinOp::Le),
            '×' => TokenKind::Op(BinOp::Mul),
            '÷' => TokenKind::Op(BinOp::Div),
            '┘' => TokenKind::Op(BinOp::Frac),
            '∠' => TokenKind::Op(BinOp::Polar),
            ';' => TokenKind::Semicolon,
            'π' => TokenKind::Const(ConstName::Pi),
            '√' => TokenKind::Func(FuncName::Sqrt),
            '∛' => TokenKind::Func(FuncName::Cbrt),
            '²' => TokenKind::Postfix(Postfix::Square),
            '³' => TokenKind::Postfix(Postfix::Cube),
            '!' => TokenKind::Postfix(Postfix::Fact),
            '%' => TokenKind::Postfix(Postfix::Percent),
            '+' => TokenKind::Op(BinOp::Add),
            '-' => TokenKind::Op(BinOp::Sub),
            '*' => TokenKind::Op(BinOp::Mul),
            '/' => TokenKind::Op(BinOp::Div),
            '=' => TokenKind::Op(BinOp::Eq),
            '<' => TokenKind::Op(BinOp::Lt),
            '>' => TokenKind::Op(BinOp::Gt),
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            ',' => TokenKind::Comma,
            '?' => TokenKind::Input,
            ':' => TokenKind::Colon,
            '^' => TokenKind::Pow,
            _ => {
                return Err(CalcError::syntax(
                    format!("unexpected character `{c}`"),
                    self.byte,
                ));
            }
        };
        self.advance();
        // `⁻¹` is a two-codepoint sequence; handle it before plain superscripts.
        Ok(Some(single))
    }

    fn scan_word(&mut self, start: usize) -> Result<Option<TokenKind>, CalcError> {
        // Special multi-character keys are checked before the general
        // keyword table because they combine a name with a symbol.
        for (text, kind) in [
            ("sin^-1", TokenKind::Func(FuncName::Asin)),
            ("cos^-1", TokenKind::Func(FuncName::Acos)),
            ("tan^-1", TokenKind::Func(FuncName::Atan)),
            ("sinh^-1", TokenKind::Func(FuncName::Asinh)),
            ("cosh^-1", TokenKind::Func(FuncName::Acosh)),
            ("tanh^-1", TokenKind::Func(FuncName::Atanh)),
            ("sin⁻¹", TokenKind::Func(FuncName::Asin)),
            ("cos⁻¹", TokenKind::Func(FuncName::Acos)),
            ("tan⁻¹", TokenKind::Func(FuncName::Atan)),
            ("sinh⁻¹", TokenKind::Func(FuncName::Asinh)),
            ("cosh⁻¹", TokenKind::Func(FuncName::Acosh)),
            ("tanh⁻¹", TokenKind::Func(FuncName::Atanh)),
            ("x√", TokenKind::Root),
            ("Ran#", TokenKind::Ran),
        ] {
            if self.looking_at(text) {
                self.consume_str(text);
                return Ok(Some(kind));
            }
        }

        // `M+` / `M-` are standalone commands; `M + 3` is arithmetic on M.
        if (self.looking_at("M+") || self.looking_at("M-"))
            && matches!(
                self.peek_at(2),
                None | Some(' ') | Some('\t') | Some('\n') | Some('\r') | Some(':') | Some('◢')
            )
        {
            let plus = self.peek_at(1) == Some('+');
            self.advance();
            self.advance();
            return Ok(Some(if plus {
                TokenKind::MPlus
            } else {
                TokenKind::MMinus
            }));
        }

        // Longest keyword match. This is what lets `4AC` read as
        // `4 × A × C` while `Abs` and `Ans` still read as single keys.
        const MAX_KEYWORD: usize = 12;
        for len in (1..=MAX_KEYWORD).rev() {
            if let Some(candidate) = self.slice(start, len)
                && let Some(kind) = word_kind(&candidate)
            {
                for _ in 0..len {
                    self.advance();
                }
                return Ok(Some(kind));
            }
        }

        Err(CalcError::syntax(
            format!("unknown identifier starting at `{}`", self.peek().unwrap()),
            self.byte,
        ))
    }

    /// The next `len` characters starting at index `start`, if available.
    fn slice(&self, start: usize, len: usize) -> Option<String> {
        let end = start + len;
        if end > self.chars.len() {
            return None;
        }
        Some(self.chars[start..end].iter().collect())
    }

    /// Read a `#mode NAME` directive.
    ///
    /// Returns `Ok(None)` when the input at the cursor is not a mode
    /// directive, so the caller can report a helpful error.  Accepts an
    /// optional `=` and surrounding spaces: `#mode CMPLX`, `#mode=CMPLX`.
    fn try_mode_directive(&mut self) -> Result<Option<TokenKind>, CalcError> {
        if !self.looking_at_ascii_ci("#mode") {
            return Ok(None);
        }
        for _ in 0..5 {
            self.advance();
        }
        while matches!(self.peek(), Some(' ') | Some('\t')) {
            self.advance();
        }
        if self.peek() == Some('=') {
            self.advance();
            while matches!(self.peek(), Some(' ') | Some('\t')) {
                self.advance();
            }
        }

        let start = self.i;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            self.advance();
        }
        let name: String = self.chars[start..self.i].iter().collect();
        let Some(mode) = Mode::parse(&name) else {
            return Err(CalcError::syntax(
                format!("unknown mode `{name}`; expected COMP, CMPLX, BASE, SD or REG"),
                self.byte,
            ));
        };
        Ok(Some(TokenKind::ModeDirective(mode)))
    }

    /// Does the remaining input start with `s`, ignoring ASCII case?
    fn looking_at_ascii_ci(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(k, c)| {
            self.peek_at(k)
                .is_some_and(|got| got.eq_ignore_ascii_case(&c))
        })
    }

    /// Try to read a base-tagged integer literal such as `1Fh`, `1010b`,
    /// `17o` or `42d`.  Only lowercase suffixes (and uppercase `H`) are
    /// accepted so that `2B`/`2D` still mean `2×B`/`2×D`.
    fn try_base_literal(&mut self) -> Option<TokenKind> {
        let start = self.i;
        let mut end = start;
        while end < self.chars.len() && self.chars[end].is_ascii_hexdigit() {
            end += 1;
        }
        if end == start {
            return None;
        }
        let candidate = if end < self.chars.len()
            && matches!(self.chars[end], 'h' | 'H' | 'b' | 'o' | 'd')
        {
            Some(end + 1)
        } else if end - start >= 2 && matches!(self.chars[end - 1], 'h' | 'H' | 'b' | 'o' | 'd') {
            Some(end)
        } else {
            None
        }?;
        let text: String = self.chars[start..candidate].iter().collect();
        // A tagged literal must not run straight into more identifier
        // characters. Otherwise `4disp` would lex as the decimal literal
        // `4d` followed by `isp`, breaking ASCII-mode `print(4)` output.
        if self
            .chars
            .get(candidate)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            return None;
        }
        Base::parse(&text)?;
        while self.i < candidate {
            self.advance();
        }
        Some(TokenKind::Number(text))
    }

    fn scan_number(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.advance();
        }
        if self.peek() == Some('.') {
            self.advance();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }
        // Exponent: `E` always, or `e` when clearly an exponent.
        let exp = matches!(self.peek(), Some('E'))
            || (self.peek() == Some('e')
                && (self.peek_at(1).is_some_and(|c| c.is_ascii_digit())
                    || (matches!(self.peek_at(1), Some('+') | Some('-'))
                        && self.peek_at(2).is_some_and(|c| c.is_ascii_digit()))));
        if exp {
            let mark = (self.i, self.byte);
            self.advance();
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.advance();
            }
            if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.advance();
                }
            } else {
                self.i = mark.0;
                self.byte = mark.1;
            }
        }
    }
}

fn word_kind(word: &str) -> Option<TokenKind> {
    use TokenKind::*;
    Some(match word {
        "A" => Var(VarName::A),
        "B" => Var(VarName::B),
        "C" => Var(VarName::C),
        "D" => Var(VarName::D),
        "X" => Var(VarName::X),
        "Y" => Var(VarName::Y),
        "M" => Var(VarName::M),
        "Ans" => Var(VarName::Ans),
        "pi" => Const(ConstName::Pi),
        "π" => Const(ConstName::Pi),
        "e" => Const(ConstName::E),
        "i" => Const(ConstName::I),
        "sin" => Func(FuncName::Sin),
        "cos" => Func(FuncName::Cos),
        "tan" => Func(FuncName::Tan),
        "asin" => Func(FuncName::Asin),
        "acos" => Func(FuncName::Acos),
        "atan" => Func(FuncName::Atan),
        "sinh" => Func(FuncName::Sinh),
        "cosh" => Func(FuncName::Cosh),
        "tanh" => Func(FuncName::Tanh),
        "asinh" => Func(FuncName::Asinh),
        "acosh" => Func(FuncName::Acosh),
        "atanh" => Func(FuncName::Atanh),
        "log" => Func(FuncName::Log),
        "ln" => Func(FuncName::Ln),
        "sqrt" => Func(FuncName::Sqrt),
        "cbrt" => Func(FuncName::Cbrt),
        "Abs" => Func(FuncName::Abs),
        "Pol" => Func(FuncName::Pol),
        "Rec" => Func(FuncName::Rec),
        "Rnd" => Func(FuncName::Rnd),
        "arg" => Func(FuncName::Arg),
        "Conjg" => Func(FuncName::Conjg),
        "Not" => Func(FuncName::Not),
        "Neg" => Func(FuncName::Neg),
        "Dec" => Dec,
        "Hex" => Hex,
        "Bin" => Bin,
        "Oct" => Oct,
        "n" => StatVar(SV::N),
        "sumx" => StatVar(SV::SumX),
        "sumx2" => StatVar(SV::SumX2),
        "sumy" => StatVar(SV::SumY),
        "sumy2" => StatVar(SV::SumY2),
        "sumxy" => StatVar(SV::SumXY),
        "meanx" => StatVar(SV::MeanX),
        "meany" => StatVar(SV::MeanY),
        "sigmax" => StatVar(SV::SigmaX),
        "sigmay" => StatVar(SV::SigmaY),
        "sx" => StatVar(SV::Sx),
        "sy" => StatVar(SV::Sy),
        "minx" | "minX" => StatVar(SV::MinX),
        "maxx" | "maxX" => StatVar(SV::MaxX),
        "miny" | "minY" => StatVar(SV::MinY),
        "maxy" | "maxY" => StatVar(SV::MaxY),
        "rega" | "regA" => StatVar(SV::RegA),
        "regb" | "regB" => StatVar(SV::RegB),
        "regr" | "regR" => StatVar(SV::RegR),
        "Σx" => StatVar(SV::SumX),
        "Σx²" => StatVar(SV::SumX2),
        "Σy" => StatVar(SV::SumY),
        "Σy²" => StatVar(SV::SumY2),
        "Σxy" => StatVar(SV::SumXY),
        "x̄" => StatVar(SV::MeanX),
        "ȳ" => StatVar(SV::MeanY),
        "σx" => StatVar(SV::SigmaX),
        "σy" => StatVar(SV::SigmaY),
        "nPr" => Op(BinOp::Perm),
        "nCr" => Op(BinOp::Comb),
        "and" => Op(BinOp::And),
        "or" => Op(BinOp::Or),
        "xor" => Op(BinOp::Xor),
        "xnor" => Op(BinOp::Xnor),
        "div" => Op(BinOp::Div),
        "Goto" => Goto,
        "Lbl" => Lbl,
        "If" => If,
        "Then" => Then,
        "Else" => Else,
        "IfEnd" => IfEnd,
        "For" => For,
        "To" => To,
        "Step" => Step,
        "Next" => Next,
        "While" => While,
        "WhileEnd" => WhileEnd,
        "Break" => Break,
        "ClrMemory" => ClrMemory,
        "ClrStat" => ClrStat,
        "FreqOn" => FreqOn,
        "FreqOff" => FreqOff,
        // ASCII alias for the display token `◢`, so that transpiled
        // `--ascii` output round-trips through the lexer.
        "disp" => Display,
        "Deg" => Deg,
        "Rad" => Rad,
        "Gra" => Gra,
        "Fix" => Fix,
        "Sci" => Sci,
        "Norm" => Norm,
        "DT" => DT,
        "Ran" => Ran,
        // Anything else that is not a keyword may be one of the calculator's
        // scientific constants, which are matched by ASCII name or by the
        // symbol on the display.
        _ => Const(ConstName::Physical(crate::constants::lookup(word)?.code)),
    })
}
