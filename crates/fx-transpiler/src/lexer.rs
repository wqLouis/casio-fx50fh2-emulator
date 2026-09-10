//! Tokenizer for the C-like `.fxc` source language.

use crate::error::TranspileError;
use crate::mode::Mode;

/// A lexical token. Keywords are their own variants so the parser can match
/// them without string comparisons.
#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// A bare name (variable, constant or function).
    Ident(String),
    /// A numeric literal.
    Number(f64),
    /// The leading `#mode NAME` directive.
    Mode(Mode),
    Let,
    Const,
    /// `free NAME` — release the memory holding a variable.
    Free,
    /// `unsafe_free NAME` — release it without the `goto`/`label` check.
    UnsafeFree,
    If,
    Else,
    While,
    For,
    Break,
    Goto,
    Label,
    Print,
    /// The `phys` namespace keyword (scientific constants).
    Phys,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    /// `**`, an alias for `^`.
    Power,
    /// `=`
    Assign,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semi,
    Comma,
    /// A standalone `.`, as in `phys.h`.
    Dot,
    Eof,
}

impl Tok {
    /// Short description used in parser error messages.
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(name) => format!("identifier `{name}`"),
            Tok::Number(value) => format!("number `{value}`"),
            Tok::Mode(mode) => format!("`#mode {}`", mode.name()),
            Tok::Let => "`let`".into(),
            Tok::Const => "`const`".into(),
            Tok::Free => "`free`".into(),
            Tok::UnsafeFree => "`unsafe_free`".into(),
            Tok::If => "`if`".into(),
            Tok::Else => "`else`".into(),
            Tok::While => "`while`".into(),
            Tok::For => "`for`".into(),
            Tok::Break => "`break`".into(),
            Tok::Goto => "`goto`".into(),
            Tok::Label => "`label`".into(),
            Tok::Print => "`print`".into(),
            Tok::Phys => "`phys`".into(),
            Tok::Plus => "`+`".into(),
            Tok::Minus => "`-`".into(),
            Tok::Star => "`*`".into(),
            Tok::Slash => "`/`".into(),
            Tok::Caret => "`^`".into(),
            Tok::Power => "`**`".into(),
            Tok::Assign => "`=`".into(),
            Tok::Eq => "`==`".into(),
            Tok::Ne => "`!=`".into(),
            Tok::Lt => "`<`".into(),
            Tok::Le => "`<=`".into(),
            Tok::Gt => "`>`".into(),
            Tok::Ge => "`>=`".into(),
            Tok::LParen => "`(`".into(),
            Tok::RParen => "`)`".into(),
            Tok::LBrace => "`{`".into(),
            Tok::RBrace => "`}`".into(),
            Tok::LBracket => "`[`".into(),
            Tok::RBracket => "`]`".into(),
            Tok::Semi => "`;`".into(),
            Tok::Comma => "`,`".into(),
            Tok::Dot => "`.`".into(),
            Tok::Eof => "end of input".into(),
        }
    }
}

/// A token plus its byte offset in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub pos: usize,
}

/// Tokenize `source`, appending an [`Tok::Eof`] sentinel.
pub fn lex(source: &str) -> Result<Vec<Token>, TranspileError> {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    source: &'a str,
    chars: Vec<char>,
    i: usize,
    byte: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Lexer {
            source,
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
        let ch = self.peek()?;
        self.i += 1;
        self.byte += ch.len_utf8();
        Some(ch)
    }

    fn looking_at(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(k, ch)| self.peek_at(k) == Some(ch))
    }

    fn consume(&mut self, text: &str) {
        for _ in text.chars() {
            self.advance();
        }
    }

    fn error(&self, message: impl Into<String>, offset: usize) -> TranspileError {
        TranspileError::at(self.source, message, offset)
    }

    fn run(mut self) -> Result<Vec<Token>, TranspileError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_trivia()?;
            let Some(ch) = self.peek() else {
                break;
            };
            let pos = self.byte;
            if ch.is_ascii_digit()
                || (ch == '.' && self.peek_at(1).is_some_and(|d| d.is_ascii_digit()))
            {
                let value = self.number()?;
                tokens.push(Token {
                    tok: Tok::Number(value),
                    pos,
                });
                continue;
            }
            if ch == '#' {
                // Only `#mode` is left: `#include` and the data directives are
                // resolved by the preprocessor, so neither ever reaches the
                // lexer.
                let directives_only = tokens.iter().all(|token| matches!(token.tok, Tok::Mode(_)));
                if !directives_only {
                    return Err(self.error(
                        "a `#` directive must be the first non-comment, non-blank line",
                        pos,
                    ));
                }
                let tok = self.directive()?;
                // A `#mode` configures the whole program, so a second one is
                // ambiguous.
                if matches!(tok, Tok::Mode(_)) && !tokens.is_empty() {
                    return Err(self.error(
                        "a `#mode` directive must be the first non-comment, non-blank line",
                        pos,
                    ));
                }
                tokens.push(Token { tok, pos });
                continue;
            }
            // An identifier is ASCII, except for a constant's display symbol
            // (`ħ`, `μμ`, `R∞`) directly after the `phys.` namespace. Allowing
            // Unicode everywhere would silently turn a misplaced glyph into a
            // variable: `π` alone would allocate a memory instead of being
            // rejected.
            let after_phys_dot = matches!(tokens.last().map(|t| &t.tok), Some(Tok::Dot));
            if ch.is_ascii_alphabetic() || ch == '_' || (after_phys_dot && ch.is_alphabetic()) {
                let name = self.word();
                tokens.push(Token {
                    tok: keyword(&name).unwrap_or(Tok::Ident(name)),
                    pos,
                });
                continue;
            }
            let tok = self.operator()?;
            tokens.push(Token { tok, pos });
        }
        tokens.push(Token {
            tok: Tok::Eof,
            pos: self.byte,
        });
        Ok(tokens)
    }

    /// Skip whitespace and comments (`// line` and `/* block */`).
    fn skip_trivia(&mut self) -> Result<(), TranspileError> {
        loop {
            match self.peek() {
                Some(ch) if ch.is_whitespace() => {
                    self.advance();
                }
                Some('/') if self.looking_at("//") => {
                    while let Some(ch) = self.peek() {
                        if ch == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                Some('/') if self.looking_at("/*") => {
                    let start = self.byte;
                    self.consume("/*");
                    loop {
                        if self.looking_at("*/") {
                            self.consume("*/");
                            break;
                        }
                        if self.advance().is_none() {
                            return Err(self.error("unterminated `/*` comment", start));
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn number(&mut self) -> Result<f64, TranspileError> {
        let start = self.byte;
        let start_i = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.advance();
        }
        if self.peek() == Some('.') {
            self.advance();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }
        // Exponent, but only when it really looks like one (`1e3`, `2.5E-2`).
        if matches!(self.peek(), Some('e') | Some('E')) {
            let save_i = self.i;
            let save_byte = self.byte;
            self.advance();
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.advance();
            }
            if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.advance();
                }
            } else {
                self.i = save_i;
                self.byte = save_byte;
            }
        }
        let text: String = self.chars[start_i..self.i].iter().collect();
        text.parse::<f64>()
            .map_err(|_| self.error(format!("invalid number `{text}`"), start))
    }

    fn word(&mut self) -> String {
        let start = self.i;
        // Unicode letters are identifiers so that a constant's display symbol
        // (`ħ`, `μμ`, `R∞`, …) can follow `phys.`. `∞` is the one symbol
        // character that is not itself a letter.
        while self
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '∞')
        {
            self.advance();
        }
        self.chars[start..self.i].iter().collect()
    }

    /// Read a `#` directive.
    ///
    /// The `#` has not been consumed. Leaves the cursor just past the end of
    /// the directive's line.
    fn directive(&mut self) -> Result<Tok, TranspileError> {
        debug_assert_eq!(self.peek(), Some('#'));
        let save_i = self.i;
        let save_byte = self.byte;
        self.advance();
        let word_start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.advance();
        }
        let word: String = self.chars[word_start..self.i].iter().collect();
        match word.to_ascii_lowercase().as_str() {
            "mode" => {
                self.i = save_i;
                self.byte = save_byte;
                let mode = self.mode_directive()?;
                Ok(Tok::Mode(mode))
            }
            // `#reg` used to pin a variable to a memory. Point anyone who
            // wrote one at what replaced it rather than just saying
            // "unknown directive".
            "reg" => Err(self.error(
                "`#reg` has been replaced: the transpiler allocates memories itself, and \
                 `free name;` releases one for reuse",
                save_byte,
            )),
            other => Err(self.error(format!("unknown directive `#{other}`"), save_byte)),
        }
    }

    /// Require the rest of the current line to be blank or a `//` comment.
    fn finish_directive_line(&mut self, start: usize, what: &str) -> Result<(), TranspileError> {
        self.skip_inline_space();
        match self.peek() {
            None | Some('\n') | Some('\r') => Ok(()),
            Some('/') if self.looking_at("//") => {
                while let Some(ch) = self.peek() {
                    if ch == '\n' {
                        break;
                    }
                    self.advance();
                }
                Ok(())
            }
            _ => Err(self.error(format!("unexpected text after {what}"), start)),
        }
    }
    /// Read the body of a `#mode NAME` directive.
    ///
    /// The `#` has not been consumed yet. Accepts an optional `=` and
    /// surrounding spaces: `#mode CMPLX`, `#mode=cmplx`. The rest of the line
    /// must be blank or a `//` comment.
    fn mode_directive(&mut self) -> Result<Mode, TranspileError> {
        let start = self.byte;
        debug_assert_eq!(self.peek(), Some('#'));
        self.advance();

        let word_start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.advance();
        }
        let directive: String = self.chars[word_start..self.i].iter().collect();
        if !directive.eq_ignore_ascii_case("mode") {
            return Err(self.error(format!("unknown directive `#{directive}`"), start));
        }

        self.skip_inline_space();
        if self.peek() == Some('=') {
            self.advance();
            self.skip_inline_space();
        }

        let name_start = self.byte;
        let name_i = self.i;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            self.advance();
        }
        let name: String = self.chars[name_i..self.i].iter().collect();
        if name.is_empty() {
            return Err(self.error("expected a mode name after `#mode`", name_start));
        }
        let Some(mode) = Mode::parse(&name) else {
            return Err(self.error(
                format!("unknown mode `{name}`; expected COMP, CMPLX, BASE, SD or REG"),
                name_start,
            ));
        };

        self.finish_directive_line(start, "the mode name")?;
        Ok(mode)
    }

    /// Skip spaces and tabs, but not newlines.
    fn skip_inline_space(&mut self) {
        while matches!(self.peek(), Some(' ') | Some('\t')) {
            self.advance();
        }
    }

    fn operator(&mut self) -> Result<Tok, TranspileError> {
        let pos = self.byte;
        for (text, tok) in [
            ("**", Tok::Power),
            ("==", Tok::Eq),
            ("!=", Tok::Ne),
            ("<=", Tok::Le),
            (">=", Tok::Ge),
        ] {
            if self.looking_at(text) {
                self.consume(text);
                return Ok(tok);
            }
        }
        let ch = self.peek().unwrap();
        let tok = match ch {
            '+' => Tok::Plus,
            '-' => Tok::Minus,
            '*' => Tok::Star,
            '/' => Tok::Slash,
            '^' => Tok::Caret,
            '=' => Tok::Assign,
            '<' => Tok::Lt,
            '>' => Tok::Gt,
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            ';' => Tok::Semi,
            ',' => Tok::Comma,
            '.' => Tok::Dot,
            _ => return Err(self.error(format!("unexpected character `{ch}`"), pos)),
        };
        self.advance();
        Ok(tok)
    }
}

fn keyword(word: &str) -> Option<Tok> {
    Some(match word {
        "let" => Tok::Let,
        "const" => Tok::Const,
        "free" => Tok::Free,
        "unsafe_free" => Tok::UnsafeFree,
        "if" => Tok::If,
        "else" => Tok::Else,
        "while" => Tok::While,
        "for" => Tok::For,
        "break" => Tok::Break,
        "goto" => Tok::Goto,
        "label" => Tok::Label,
        "print" => Tok::Print,
        "phys" => Tok::Phys,
        _ => return None,
    })
}
