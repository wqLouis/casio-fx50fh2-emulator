//! Measuring how many program keys a PRGM listing costs.
//!
//! ## Why this exists
//!
//! The fx-50FH II has **680 bytes of program storage, shared by all four
//! program areas** (P1–P4). The machine stores a program as one byte per key —
//! `sin(` is a single key, `→` is a single key, and each digit of `12` is its
//! own key. So the number of keys is the number the memory display counts down,
//! and it is the number every optimisation has to reduce.
//!
//! Counting keys therefore means **tokenising the PRGM text the way the
//! machine does**, not counting characters: `log(` is 1 key, not 4, and `12` is
//! 2 keys, not 1. The interpreter's own PRGM lexer (`src/lexer.rs` in the
//! repository root) is the authority on the key set; this module mirrors it
//! while staying free of the interpreter dependency, since `fx-transpiler`
//! builds with `--no-default-features` and **zero third-party dependencies**.
//!
//! ## The key set, as `src/lexer.rs` defines it
//!
//! One key is one of:
//!
//! * a multi-character keyword the lexer folds into a single token —
//!   `ClrMemory`, `WhileEnd`, `sin`, `abs`, the statistical variables
//!   (`Σx`, `x̄`, `sumx`, …) and the 40 scientific constants (`R∞`, `λcp`,
//!   `sigma`, …);
//! * a **prefix function together with the opening parenthesis it inserts**.
//!   The lexer emits the `(` as a separate [`LParen`] token because the parser
//!   needs it, but the machine's `log` key inserts `log(`, so `log(` is one
//!   key. The same applies to the glyph forms (`√(`), the parenthetical
//!   binary keys (`^(`, `x√(`) and `10^(`, `e^(`. See
//!   `docs/LANGUAGE.md`'s "Prefix functions" and "Parenthetical binary" rows,
//!   and `TokenKind::Pow`/`Root`, which describe themselves as `"^("` and
//!   `"x√("`;
//! * a single character key: `→`, `◢`, `≠`, `≤`, `×`, `π`, `?`, `)`, `,`, …
//! * a signed/unsigned **number**, except that each digit (and the decimal
//!   point) is its own key: `12` is 2 keys, `1.5` is 3, `1E3` is 3.
//!
//! Multi-character ASCII aliases count as the one key they stand for:
//! `sqrt(` = `√(`, `disp` = `◢`, `->` = `→`, `<>` = `≠`, `<=` = `≤`,
//! `>=` = `≥`, `pi` = `π`, `*` = `×`, `/` = `÷`.
//!
//! ## Worked examples
//!
//! All counts below are derived from the rules above (`◢` is the display key,
//! `→` the assignment key):
//!
//! | listing | keys | breakdown |
//! | --- | --- | --- |
//! | `1→A` | 3 | `1` + `→` + `A` |
//! | `10◢` | 3 | `1` + `0` + `◢` |
//! | `A×2◢` | 4 | `A` + `×` + `2` + `◢` |
//! | `log(2)◢` | 4 | `log(` + `2` + `)` + `◢` |
//! | `?→A` | 3 | `?` + `→` + `A` |
//!
//! ## The separator ambiguity, resolved
//!
//! The machine stores one byte per *key*, and the statement separator is where
//! the manual's accounting is least explicit. The stub contract says a
//! listing's blank lines and indentation must not change the count, so this
//! module treats a **newline as free**: it is a layout artefact of the editor,
//! not a stored key. The emitted listing puts one statement per line and never
//! needs `:`, so emitted programs pay nothing for statement boundaries. A
//! literal `:` is a real key on the keyboard, so it *is* counted (one key) and
//! is also treated as a statement boundary. This keeps the invariant the
//! caller cares about — adding blank lines, wrapping or indenting a listing
//! does not move `Size::keys` — while still charging for the `:` key when a
//! listing actually contains one.
//!
//! A `#mode NAME` header emitted by [`crate::transpile`] is a *host* directive
//! that the MODE key would have set before the program was typed in; it is not
//! stored in program memory, so it is neither counted nor treated as a
//! statement.
//!
//! ## What is reported
//!
//! [`measure`] returns the total [`Size::keys`], the number of statements
//! ([`Size::statements`], i.e. non-blank lines or `:`-separated fragments) and
//! the cost of the largest single statement ([`Size::largest`]), so a report
//! can point at the line that needs work.
//!
//! [`Size::fits`] is the question a programmer actually asks: does this program
//! fit in the 680-byte store?

/// The size of a PRGM listing, in the keys the machine stores.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Size {
    /// Total keys, which is the number of bytes the program occupies.
    pub keys: usize,
    /// Statements, i.e. lines of the listing.
    pub statements: usize,
    /// The cost of the largest single statement.
    pub largest: usize,
}

impl Size {
    /// The capacity of the fx-50FH II program store, in bytes.
    ///
    /// All four program areas (`P1`–`P4`) share this one store.
    pub const CAPACITY: usize = 680;

    /// Whether the program fits in the machine's program store.
    pub fn fits(&self) -> bool {
        self.keys <= Self::CAPACITY
    }

    /// How much of the store is left, or `None` when it does not fit.
    pub fn remaining(&self) -> Option<usize> {
        Self::CAPACITY.checked_sub(self.keys)
    }
}

/// Measure the keys `prgm` would occupy.
///
/// `prgm` is a listing as produced by [`crate::transpile`], in either the
/// glyph or the ASCII style. Whitespace, line breaks and `//` comments are
/// free; a `#mode` header is free; see the module docs for the rules and the
/// separator ambiguity.
pub fn measure(prgm: &str) -> Size {
    let mut size = Size::default();
    let mut current = 0usize;
    let mut scanner = Scanner::new(prgm);
    while let Some(c) = scanner.peek() {
        if c == '\n' || c == '\r' {
            // A line break only ends a statement; it is not a stored key.
            scanner.advance();
            settle(&mut size, &mut current);
        } else if c.is_whitespace() {
            scanner.advance();
        } else if c == '/' && scanner.looking_at("//") {
            // Emitted listings contain no comments, but the contract promises
            // that one would not be counted.
            scanner.skip_line();
        } else if c == '#' {
            // `#mode …`: a host directive, not a stored keystroke.
            scanner.skip_line();
        } else if c == ':' {
            // `:` is a real key, so it is charged, then ends the statement.
            scanner.advance();
            current += 1;
            settle(&mut size, &mut current);
        } else {
            current += scanner.scan_key();
        }
    }
    settle(&mut size, &mut current);
    size
}

/// Fold the statement in progress into `size` and start a new one.
fn settle(size: &mut Size, current: &mut usize) {
    if *current > 0 {
        size.statements += 1;
        size.keys += *current;
        size.largest = size.largest.max(*current);
        *current = 0;
    }
}

/// A character cursor over a PRGM listing.
///
/// This deliberately re-implements the token scan of the interpreter's
/// `src/lexer.rs` rather than depending on it: `fx-transpiler` must keep
/// building with `--no-default-features` and no third-party crates, and the
/// interpreter's lexer also lives in a crate the transpiler cannot reach.
struct Scanner {
    chars: Vec<char>,
    i: usize,
}

/// The key spellings that combine a name with a symbol. Mirrors the special
/// cases at the top of `src/lexer.rs`'s `scan_word`. The flag says whether the
/// opening parenthesis belongs to the key.
const SPECIAL_WORDS: &[(&str, bool)] = &[
    ("sin^-1", true),
    ("cos^-1", true),
    ("tan^-1", true),
    ("sinh^-1", true),
    ("cosh^-1", true),
    ("tanh^-1", true),
    ("sin⁻¹", true),
    ("cos⁻¹", true),
    ("tan⁻¹", true),
    ("sinh⁻¹", true),
    ("cosh⁻¹", true),
    ("tanh⁻¹", true),
    ("x√", true),
    ("Ran#", false),
];

/// The complex-format setup keys, copied from `src/lexer.rs`.
const COMPLEX_FORMATS: &[&str] = &["▶a+b𝑖", "▶a+b𝒾", "▶a+bi", "▶r∠θ", ">a+bi", ">rangle"];

/// The postfix power keys. Each is a single key; `^( ` is handled separately.
const POSTFIX_POWERS: &[&str] = &["^-1", "⁻¹", "^2", "^3"];

/// Multi-character symbolic aliases. Each is a single key.
const SYMBOLIC_TOKENS: &[&str] = &["->", "=>", "<=", ">=", "<>"];

/// The longest keyword the interpreter's lexer recognises.
const MAX_KEYWORD: usize = 12;

impl Scanner {
    fn new(source: &str) -> Self {
        Scanner {
            chars: source.chars().collect(),
            i: 0,
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
        Some(c)
    }

    /// Does the remaining input start with `s`?
    fn looking_at(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(k, c)| self.peek_at(k) == Some(c))
    }

    fn consume(&mut self, count: usize) {
        self.i = (self.i + count).min(self.chars.len());
    }

    fn consume_str(&mut self, s: &str) {
        self.consume(s.chars().count());
    }

    /// Consume a `(` that belongs to the key just scanned, if present.
    fn absorb_open_paren(&mut self) {
        if self.peek() == Some('(') {
            self.advance();
        }
    }

    /// Skip to (but not past) the end of the current line.
    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' || c == '\r' {
                break;
            }
            self.advance();
        }
    }

    /// Count the keys in the token beginning at the cursor and consume it.
    ///
    /// The order of the checks mirrors `src/lexer.rs`'s `scan_token` so that
    /// the same text is split into the same keys: `10^` before a base literal
    /// before a number, word before number, postfix powers before a bare `^`,
    /// and the multi-character aliases before the single characters.
    fn scan_key(&mut self) -> usize {
        // `10^` is the only key beginning with a digit.
        if self.looking_at("10^") {
            self.consume_str("10^");
            self.absorb_open_paren();
            return 1;
        }
        // `e^` is a prefix-function key too. The lexer reaches the same token
        // as `e` + `^` (its parser regains the parenthesis), but on the
        // machine `e^x` inserts `e^(` as one press, so it is one key here.
        if self.looking_at("e^") {
            self.consume_str("e^");
            self.absorb_open_paren();
            return 1;
        }
        for text in COMPLEX_FORMATS {
            if self.looking_at(text) {
                self.consume_str(text);
                return 1;
            }
        }
        // A base-tagged literal (`1Fh`, `1010b`, `17o`, `42d`). The tag is an
        // interpreter annotation selecting a base, not a keystroke; on the
        // machine the digits are typed in a base selected up front, so only
        // the digits are charged.
        if self.peek().is_some_and(|c| c.is_ascii_hexdigit())
            && let Some((consumed, digits)) = self.try_base_literal()
        {
            self.consume(consumed);
            return digits;
        }
        if self.peek().is_some_and(|c| c.is_alphabetic()) {
            return self.scan_word();
        }
        if self.peek().is_some_and(|c| c.is_ascii_digit())
            || (self.peek() == Some('.') && self.peek_at(1).is_some_and(|d| d.is_ascii_digit()))
        {
            return self.scan_number();
        }
        for text in POSTFIX_POWERS {
            if self.looking_at(text) {
                self.consume_str(text);
                return 1;
            }
        }
        for text in SYMBOLIC_TOKENS {
            if self.looking_at(text) {
                self.consume_str(text);
                return 1;
            }
        }
        self.scan_single()
    }

    /// Mirror of `src/lexer.rs`'s `scan_word`.
    fn scan_word(&mut self) -> usize {
        for (text, absorbs_paren) in SPECIAL_WORDS {
            if self.looking_at(text) {
                self.consume_str(text);
                if *absorbs_paren {
                    self.absorb_open_paren();
                }
                return 1;
            }
        }

        // `M+` / `M-` are standalone commands; `M + 3` is arithmetic on `M`.
        if (self.looking_at("M+") || self.looking_at("M-"))
            && matches!(
                self.peek_at(2),
                None | Some(' ') | Some('\t') | Some('\n') | Some('\r') | Some(':') | Some('◢')
            )
        {
            self.consume(2);
            return 1;
        }

        // Longest keyword match, exactly as the lexer does it, so `sinh` is
        // one key rather than `sin` + `h` and `sumx2` is one key.
        let remaining = self.chars.len() - self.i;
        for len in (1..=MAX_KEYWORD.min(remaining)).rev() {
            let candidate: String = self.chars[self.i..self.i + len].iter().collect();
            if let Some(absorbs_paren) = key_kind(&candidate) {
                self.consume(len);
                if absorbs_paren {
                    self.absorb_open_paren();
                }
                return 1;
            }
        }

        // Emitted PRGM never contains an unknown identifier, so this path is
        // only a safety net for hand-written input. Charge one key per
        // character to guarantee progress and avoid under-counting.
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            self.advance();
        }
        if self.i == start {
            self.advance();
            return 1;
        }
        self.i - start
    }

    /// Mirror of `src/lexer.rs`'s `try_base_literal`.
    ///
    /// Returns `(characters to consume, digit keys)` when the cursor is on a
    /// valid tagged literal such as `1Fh`.
    fn try_base_literal(&self) -> Option<(usize, usize)> {
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
            end + 1
        } else if end - start >= 2 && matches!(self.chars[end - 1], 'h' | 'H' | 'b' | 'o' | 'd') {
            end
        } else {
            return None;
        };
        // A tagged literal must not run into more identifier characters, or
        // `4disp` would lex as `4d` followed by `isp`.
        if self
            .chars
            .get(candidate)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            return None;
        }
        let suffix = self.chars[candidate - 1];
        let body = &self.chars[start..candidate - 1];
        let radix = match suffix {
            'h' | 'H' => 16,
            'b' => 2,
            'o' => 8,
            'd' => 10,
            _ => return None,
        };
        if body.is_empty() || body.iter().any(|c| c.to_digit(radix).is_none()) {
            return None;
        }
        Some((candidate - start, body.len()))
    }

    /// Count the keys in a numeric literal and consume it.
    ///
    /// Mirrors `src/lexer.rs`'s `scan_number` for what it consumes, but
    /// charges one key per digit, per decimal point and per exponent marker.
    fn scan_number(&mut self) -> usize {
        let mut keys = 0;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.advance();
            keys += 1;
        }
        if self.peek() == Some('.') {
            self.advance();
            keys += 1;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
                keys += 1;
            }
        }
        // `E` always starts an exponent; `e` only when a digit (or sign and
        // digit) follows, so `2e` stays a number followed by Euler's number.
        let exponent = self.peek() == Some('E')
            || (self.peek() == Some('e')
                && (self.peek_at(1).is_some_and(|c| c.is_ascii_digit())
                    || (matches!(self.peek_at(1), Some('+') | Some('-'))
                        && self.peek_at(2).is_some_and(|c| c.is_ascii_digit()))));
        if exponent {
            let mark = self.i;
            let mut added = 0;
            self.advance();
            added += 1;
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.advance();
                added += 1;
            }
            if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.advance();
                    added += 1;
                }
                keys += added;
            } else {
                // The lexer rolls the marker back when no exponent digit
                // follows; the key was not really part of the number.
                self.i = mark;
            }
        }
        keys
    }

    /// Consume one character key. `^(`, `√(` and `∛(` swallow the opening
    /// parenthesis that the machine's key inserts.
    fn scan_single(&mut self) -> usize {
        let Some(c) = self.advance() else {
            return 0;
        };
        if matches!(c, '^' | '√' | '∛') {
            self.absorb_open_paren();
        }
        1
    }
}

/// Classify a word as a single key.
///
/// Returns `Some(true)` when the word is a prefix function that owns the
/// opening parenthesis after it, and `Some(false)` for every other single-key
/// word. `None` means the word is not a key on its own.
fn key_kind(word: &str) -> Option<bool> {
    let absorbs_paren = match word {
        // Memories and elementary constants.
        "A" | "B" | "C" | "D" | "X" | "Y" | "M" | "Ans" => false,
        "pi" | "π" | "e" | "i" => false,
        // Prefix functions. Each owns the `(` the machine inserts.
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh"
        | "acosh" | "atanh" | "log" | "ln" | "sqrt" | "cbrt" | "Abs" | "Pol" | "Rec" | "Rnd"
        | "arg" | "Conjg" | "Not" | "Neg" => true,
        // Infix and postfix keys.
        "nPr" | "nCr" | "and" | "or" | "xor" | "xnor" | "div" => false,
        // Program commands.
        "Goto" | "Lbl" | "If" | "Then" | "Else" | "IfEnd" | "For" | "To" | "Step" | "Next"
        | "While" | "WhileEnd" | "Break" => false,
        // Setup and data commands.
        "ClrMemory" | "ClrStat" | "FreqOn" | "FreqOff" | "Deg" | "Rad" | "Gra" | "Fix" | "Sci"
        | "Norm" | "Dec" | "Hex" | "Bin" | "Oct" | "DT" | "Ran" | "disp" => false,
        _ => {
            if is_stat_var(word) || crate::constants::lookup(word).is_some() {
                false
            } else {
                return None;
            }
        }
    };
    Some(absorbs_paren)
}

/// Is `word` the glyph or ASCII spelling of a statistical variable?
fn is_stat_var(word: &str) -> bool {
    crate::ast::StatVar::ALL
        .iter()
        .any(|var| var.glyph() == word || var.ascii() == word)
}
