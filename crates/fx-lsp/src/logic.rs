//! Pure, testable language-server logic.
//!
//! Nothing in this module does I/O or depends on async.  It maps source text
//! to LSP structures: byte offsets to [`Position`]s, diagnostics produced by
//! the core lexer/parser, completion items, hover text and document symbols.

// The core enums are not `#[non_exhaustive]`, so the wildcard arms that exist
// purely for forward compatibility are unreachable today.  Keep them (so a new
// core variant does not break this crate) and silence the lint.
#![allow(unreachable_patterns)]

use casio_fx50fh2::CalcError;
use casio_fx50fh2::lexer::lex;
use casio_fx50fh2::parser::parse;
use casio_fx50fh2::token::{BinOp, ConstName, FuncName, Postfix, Token, TokenKind, VarName};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol,
    Documentation, Hover, HoverContents, InsertTextFormat, MarkupContent, MarkupKind,
    NumberOrString, Position, Range, SymbolKind,
};

// ---------------------------------------------------------------------------
// Position <-> byte offset
// ---------------------------------------------------------------------------

/// Convert a byte offset into `source` to an LSP [`Position`].
///
/// Lines are zero-based and `character` counts UTF-16 code units, exactly as
/// the protocol requires.  Offsets that are not on a UTF-8 boundary (which a
/// well-behaved lexer never produces) are rounded down, and offsets past the
/// end of the document are clamped.
pub fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut offset = offset.min(source.len());
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }

    let before = &source[..offset];
    let line = before.bytes().filter(|b| *b == b'\n').count() as u32;
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let character = source[line_start..offset]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum();

    Position::new(line, character)
}

/// Convert an LSP [`Position`] to a byte offset into `source`.
///
/// Returns `None` when the position lies on a line that does not exist.
pub fn position_to_offset(source: &str, position: Position) -> Option<usize> {
    let mut line_start = 0usize;
    let mut current_line = 0u32;
    while current_line < position.line {
        let rel = source[line_start..].find('\n')?;
        line_start += rel + 1;
        current_line += 1;
    }

    let line_end = source[line_start..]
        .find('\n')
        .map(|rel| line_start + rel)
        .unwrap_or(source.len());

    let mut utf16 = 0u32;
    for (i, c) in source[line_start..line_end].char_indices() {
        if utf16 >= position.character {
            return Some(line_start + i);
        }
        utf16 += c.len_utf16() as u32;
    }
    // Past the end of the line: clamp to the line end.
    Some(line_end)
}

/// The byte offset of the next character boundary strictly after `offset`.
fn next_char_boundary(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    if offset >= source.len() {
        return source.len();
    }
    let mut next = offset + 1;
    while next < source.len() && !source.is_char_boundary(next) {
        next += 1;
    }
    next
}

/// A one-character range starting at `offset`.
pub fn range_for_offset(source: &str, offset: usize) -> Range {
    let start = offset_to_position(source, offset);
    let end = offset_to_position(source, next_char_boundary(source, offset));
    Range::new(start, end)
}

/// The range covering `token`'s lexeme.
pub fn token_range(source: &str, token: &Token) -> Range {
    let start = offset_to_position(source, token.pos);
    let end = if token.lexeme.is_empty() {
        offset_to_position(source, token.pos)
    } else {
        offset_to_position(source, token.pos + token.lexeme.len())
    };
    Range::new(start, end)
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// The byte offset a [`CalcError`] points at, when it has one.
///
/// The wildcard arm keeps this compiling if the core adds new error variants.
pub fn error_position(err: &CalcError) -> Option<usize> {
    match err {
        CalcError::Syntax { pos, .. } => *pos,
        _ => None,
    }
}

/// Build an LSP diagnostic from a core error.
pub fn diagnostic(source: &str, err: &CalcError) -> Diagnostic {
    let offset = error_position(err).unwrap_or(0);
    Diagnostic {
        range: range_for_offset(source, offset),
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(err.label().to_string())),
        code_description: None,
        source: Some("fx-50FH II".to_string()),
        message: err.to_string(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Lex and parse `source`, returning any diagnostics.
///
/// A lex error short-circuits parsing.  A successful parse yields no
/// diagnostics; semantic errors (Math ERROR etc.) are runtime concerns and
/// are not reported by the language server.
pub fn diagnostics(source: &str) -> Vec<Diagnostic> {
    match lex(source) {
        Err(err) => vec![diagnostic(source, &err)],
        Ok(tokens) => match parse(tokens) {
            Ok(_) => Vec::new(),
            Err(err) => vec![diagnostic(source, &err)],
        },
    }
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

/// Add `item` unless an item with the same label already exists.
fn push(items: &mut Vec<CompletionItem>, item: CompletionItem) {
    if !items.iter().any(|existing| existing.label == item.label) {
        items.push(item);
    }
}

fn simple(label: &str, kind: CompletionItemKind, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        documentation: Some(Documentation::String(detail.to_string())),
        ..Default::default()
    }
}

fn function(label: &str, insert: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some(detail.to_string()),
        documentation: Some(Documentation::String(detail.to_string())),
        insert_text: Some(insert.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

/// Every completion item the server offers.
///
/// The list is deliberately context-free: the calculator keyboard is tiny, so
/// offering the whole vocabulary is cheap and predictable.  Items are
/// de-duplicated by label.
pub fn completion_items() -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // -- keywords -----------------------------------------------------------
    let keywords = [
        ("If", "conditional execution"),
        ("Then", "start of an `If` body"),
        ("Else", "alternative `If` body"),
        ("IfEnd", "end of an `If` block"),
        ("For", "counted loop"),
        ("To", "`For` upper bound"),
        ("Step", "`For` step"),
        ("Next", "end of a `For` loop"),
        ("While", "conditional loop"),
        ("WhileEnd", "end of a `While` loop"),
        ("Break", "exit the innermost loop"),
        ("Goto", "unconditional jump to a label"),
        ("Lbl", "jump label"),
        ("ClrMemory", "clear A..M and Ans"),
        ("ClrStat", "clear statistical data"),
        ("FreqOn", "turn frequency column on"),
        ("FreqOff", "turn frequency column off"),
        ("DT", "statistics data entry"),
        ("Deg", "degree angle unit"),
        ("Rad", "radian angle unit"),
        ("Gra", "gradian angle unit"),
        ("Fix", "fixed decimal display"),
        ("Sci", "scientific display"),
        ("Norm", "normal display"),
    ];
    for (label, detail) in keywords {
        push(
            &mut items,
            simple(label, CompletionItemKind::KEYWORD, detail),
        );
    }

    // -- functions ----------------------------------------------------------
    let functions: &[(&str, &str, &str)] = &[
        ("sin(", "sin($0)", "sine"),
        ("cos(", "cos($0)", "cosine"),
        ("tan(", "tan($0)", "tangent"),
        ("sin^-1(", "sin^-1($0)", "inverse sine (ASCII)"),
        ("cos^-1(", "cos^-1($0)", "inverse cosine (ASCII)"),
        ("tan^-1(", "tan^-1($0)", "inverse tangent (ASCII)"),
        ("sinh(", "sinh($0)", "hyperbolic sine"),
        ("cosh(", "cosh($0)", "hyperbolic cosine"),
        ("tanh(", "tanh($0)", "hyperbolic tangent"),
        ("sinh^-1(", "sinh^-1($0)", "inverse hyperbolic sine"),
        ("cosh^-1(", "cosh^-1($0)", "inverse hyperbolic cosine"),
        ("tanh^-1(", "tanh^-1($0)", "inverse hyperbolic tangent"),
        (
            "log(",
            "log($0)",
            "common logarithm; `log(a,b)` is base-a log",
        ),
        ("ln(", "ln($0)", "natural logarithm"),
        ("√(", "√($0)", "square root"),
        ("∛(", "∛($0)", "cube root"),
        ("Abs(", "Abs($0)", "absolute value / modulus"),
        ("arg(", "arg($0)", "argument of a complex number"),
        ("Conjg(", "Conjg($0)", "complex conjugate"),
        ("Pol(", "Pol($0)", "rectangular to polar; writes X and Y"),
        ("Rec(", "Rec($0)", "polar to rectangular; writes X and Y"),
        ("Rnd(", "Rnd($0)", "round to display digits"),
        ("Ran#", "Ran#", "pseudo-random number"),
        ("10^(", "10^($0)", "power of ten"),
        ("e^(", "e^($0)", "power of e"),
    ];
    for (label, insert, detail) in functions {
        push(&mut items, function(label, insert, detail));
    }

    // -- variables and constants -------------------------------------------
    for (label, detail) in [
        ("A", "memory A"),
        ("B", "memory B"),
        ("C", "memory C"),
        ("D", "memory D"),
        ("X", "memory X"),
        ("Y", "memory Y"),
        ("M", "independent memory M"),
        ("Ans", "last answer"),
    ] {
        push(
            &mut items,
            simple(label, CompletionItemKind::VARIABLE, detail),
        );
    }
    for (label, detail) in [
        ("π", "the constant pi"),
        ("e", "Euler's number"),
        ("i", "imaginary unit"),
    ] {
        push(
            &mut items,
            simple(label, CompletionItemKind::CONSTANT, detail),
        );
    }

    // -- operators ----------------------------------------------------------
    let operators = [
        ("+", "addition"),
        ("-", "subtraction"),
        ("×", "multiplication"),
        ("÷", "division"),
        ("^(", "power"),
        ("x²", "square"),
        ("x³", "cube"),
        ("x⁻¹", "reciprocal"),
        ("!", "factorial"),
        ("%", "percent"),
        ("nPr", "permutations"),
        ("nCr", "combinations"),
        ("and", "logical/bitwise and"),
        ("or", "logical/bitwise or"),
        ("xor", "logical/bitwise xor"),
        ("xnor", "logical/bitwise xnor"),
    ];
    for (label, detail) in operators {
        push(
            &mut items,
            simple(label, CompletionItemKind::OPERATOR, detail),
        );
    }

    // -- ASCII aliases ------------------------------------------------------
    let aliases = [
        ("->", "assignment, glyph `→`"),
        ("=>", "conditional jump, glyph `⇒`"),
        ("disp", "display result, glyph `◢`"),
        ("<>", "not equal, glyph `≠`"),
        ("<=", "less than or equal, glyph `≤`"),
        (">=", "greater than or equal, glyph `≥`"),
        (">a+bi", "set complex display to Cartesian `a+b𝑖`"),
        (">rangle", "set complex display to polar `r∠θ`"),
    ];
    for (label, detail) in aliases {
        push(
            &mut items,
            simple(label, CompletionItemKind::OPERATOR, detail),
        );
    }

    items
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

/// Find the token whose lexeme contains `offset`, if any.
fn token_at(tokens: &[Token], offset: usize) -> Option<&Token> {
    tokens.iter().find(|token| {
        !matches!(token.kind, TokenKind::Eof)
            && !token.lexeme.is_empty()
            && token.pos <= offset
            && offset < token.pos + token.lexeme.len()
    })
}

/// Hover information for the token under `position`.
pub fn hover(source: &str, position: Position) -> Option<Hover> {
    let offset = position_to_offset(source, position)?;
    let tokens = lex(source).ok()?;
    let token = token_at(&tokens, offset)?;
    let value = describe_token(&token.kind)?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(token_range(source, token)),
    })
}

fn var_description(var: &VarName) -> &'static str {
    match var {
        VarName::A => "Memory A",
        VarName::B => "Memory B",
        VarName::C => "Memory C",
        VarName::D => "Memory D",
        VarName::X => "Memory X",
        VarName::Y => "Memory Y",
        VarName::M => "Independent memory M",
        VarName::Ans => "The last computed answer",
        _ => "Calculator memory",
    }
}

fn const_description(c: &ConstName) -> &'static str {
    match c {
        ConstName::Pi => "The constant pi (3.14159…)",
        ConstName::E => "Euler's number (2.71828…)",
        ConstName::I => "The imaginary unit i",
        _ => "Mathematical constant",
    }
}

fn func_description(f: &FuncName) -> &'static str {
    use FuncName::*;
    match f {
        Sin => "Sine",
        Cos => "Cosine",
        Tan => "Tangent",
        Asin => "Inverse sine",
        Acos => "Inverse cosine",
        Atan => "Inverse tangent",
        Sinh => "Hyperbolic sine",
        Cosh => "Hyperbolic cosine",
        Tanh => "Hyperbolic tangent",
        Asinh => "Inverse hyperbolic sine",
        Acosh => "Inverse hyperbolic cosine",
        Atanh => "Inverse hyperbolic tangent",
        Log => "Logarithm; `log(a,b)` is log base a",
        Ln => "Natural logarithm",
        Sqrt => "Square root",
        Cbrt => "Cube root",
        TenPow => "Power of ten",
        EPow => "Power of e",
        Abs => "Absolute value / complex modulus",
        Pol => "Rectangular to polar conversion (writes X and Y)",
        Rec => "Polar to rectangular conversion (writes X and Y)",
        Rnd => "Round to the current display precision",
        Arg => "Argument (angle) of a complex number",
        Conjg => "Complex conjugate",
        _ => "Calculator function",
    }
}

fn binop_description(op: &BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add => "Addition",
        Sub => "Subtraction",
        Mul => "Multiplication",
        Div => "Division",
        Frac => "Fraction",
        Perm => "Permutations (nPr)",
        Comb => "Combinations (nCr)",
        Eq => "Equality test (1 when true, 0 when false)",
        Ne => "Inequality test",
        Gt => "Greater-than test",
        Lt => "Less-than test",
        Ge => "Greater-than-or-equal test",
        Le => "Less-than-or-equal test",
        And => "Logical / bitwise and",
        Or => "Logical / bitwise or",
        Xor => "Logical / bitwise exclusive or",
        Xnor => "Logical / bitwise exclusive nor",
        _ => "Binary operator",
    }
}

fn postfix_description(p: &Postfix) -> &'static str {
    match p {
        Postfix::Inverse => "Reciprocal (x⁻¹)",
        Postfix::Square => "Square (x²)",
        Postfix::Cube => "Cube (x³)",
        Postfix::Fact => "Factorial (x!)",
        Postfix::Percent => "Percent (x%)",
        _ => "Postfix operator",
    }
}

/// Markdown description for a single token, or `None` when it is not worth
/// hovering.  Unknown (newly added) token kinds fall through to `None`.
fn describe_token(kind: &TokenKind) -> Option<String> {
    let text = match kind {
        TokenKind::Number(_) => "**Number**\n\nA numeric literal.".to_string(),
        TokenKind::Var(v) => {
            format!("**`{}`** — variable\n\n{}", v.name(), var_description(v))
        }
        TokenKind::Const(c) => {
            let name = match c {
                ConstName::Pi => "π".to_string(),
                ConstName::E => "e".to_string(),
                ConstName::I => "i".to_string(),
                _ => "?".to_string(),
            };
            format!("**`{name}`** — constant\n\n{}", const_description(c))
        }
        TokenKind::Func(f) => format!("**Function**\n\n{}", func_description(f)),
        TokenKind::Postfix(p) => {
            format!("**Postfix operator**\n\n{}", postfix_description(p))
        }
        TokenKind::Pow => {
            "**`^(`** — power\n\nRaises the value on the left to the parenthesised exponent."
                .to_string()
        }
        TokenKind::Root => {
            "**`x√(`** — root\n\nTakes the `x`-th root of the parenthesised radicand.".to_string()
        }
        TokenKind::Op(op) => format!("**Operator**\n\n{}", binop_description(op)),
        TokenKind::LParen => "**`(`** — open parenthesis".to_string(),
        TokenKind::RParen => "**`)`** — close parenthesis".to_string(),
        TokenKind::Comma => "**`,`** — argument separator".to_string(),
        TokenKind::Input => {
            "**`?`** — input prompt\n\nReads a number from the user when the program runs."
                .to_string()
        }
        TokenKind::Assign => {
            "**`→`** — assignment\n\nStores the value on the left into the variable on the right."
                .to_string()
        }
        TokenKind::Colon => "**`:`** — statement separator".to_string(),
        TokenKind::Display => {
            "**`◢`** — display\n\nPauses and shows the preceding result.".to_string()
        }
        TokenKind::CondJump => "**`⇒`** — conditional jump".to_string(),
        TokenKind::Goto => "**`Goto`** — unconditional jump to a label".to_string(),
        TokenKind::Lbl => "**`Lbl`** — jump label".to_string(),
        TokenKind::If => "**`If`** — begin a conditional".to_string(),
        TokenKind::Then => "**`Then`** — begin the `If` body".to_string(),
        TokenKind::Else => "**`Else`** — alternative `If` body".to_string(),
        TokenKind::IfEnd => "**`IfEnd`** — end of an `If` block".to_string(),
        TokenKind::For => "**`For`** — counted loop".to_string(),
        TokenKind::To => "**`To`** — `For` upper bound".to_string(),
        TokenKind::Step => "**`Step`** — `For` increment".to_string(),
        TokenKind::Next => "**`Next`** — end of a `For` loop".to_string(),
        TokenKind::While => "**`While`** — conditional loop".to_string(),
        TokenKind::WhileEnd => "**`WhileEnd`** — end of a `While` loop".to_string(),
        TokenKind::Break => "**`Break`** — exit the innermost loop".to_string(),
        TokenKind::MPlus => "**`M+`** — add to independent memory".to_string(),
        TokenKind::MMinus => "**`M-`** — subtract from independent memory".to_string(),
        TokenKind::ClrMemory => "**`ClrMemory`** — clear memories".to_string(),
        TokenKind::ClrStat => "**`ClrStat`** — clear statistical data".to_string(),
        TokenKind::FreqOn => "**`FreqOn`** — enable the frequency column".to_string(),
        TokenKind::FreqOff => "**`FreqOff`** — disable the frequency column".to_string(),
        TokenKind::Deg => "**`Deg`** — degrees".to_string(),
        TokenKind::Rad => "**`Rad`** — radians".to_string(),
        TokenKind::Gra => "**`Gra`** — gradians".to_string(),
        TokenKind::Fix => "**`Fix`** — fixed decimal display".to_string(),
        TokenKind::Sci => "**`Sci`** — scientific display".to_string(),
        TokenKind::Norm => "**`Norm`** — normal display".to_string(),
        TokenKind::DT => "**`DT`** — statistics data entry".to_string(),
        TokenKind::Ran => "**`Ran#`** — pseudo-random number".to_string(),
        TokenKind::Eof => return None,
        _ => return None,
    };
    Some(text)
}

// ---------------------------------------------------------------------------
// Document symbols
// ---------------------------------------------------------------------------

/// One symbol per `Lbl` marker in the program.
///
/// Tokens are scanned directly rather than walking the AST so that symbols
/// still work while the rest of the document has a syntax error.
#[allow(deprecated)]
pub fn document_symbols(source: &str) -> Vec<DocumentSymbol> {
    let Ok(tokens) = lex(source) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if !matches!(token.kind, TokenKind::Lbl) {
            continue;
        }
        let (name, range) = match tokens.get(i + 1) {
            Some(next) => match &next.kind {
                TokenKind::Number(digit) => (
                    format!("Lbl {digit}"),
                    Range::new(
                        offset_to_position(source, token.pos),
                        offset_to_position(source, next.pos + next.lexeme.len()),
                    ),
                ),
                _ => ("Lbl".to_string(), token_range(source, token)),
            },
            None => ("Lbl".to_string(), token_range(source, token)),
        };

        symbols.push(DocumentSymbol {
            name,
            detail: Some("jump label".to_string()),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children: None,
        });
    }
    symbols
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> Vec<String> {
        completion_items()
            .into_iter()
            .map(|item| item.label)
            .collect()
    }

    #[test]
    fn offset_to_position_ascii() {
        let src = "AB\nCD";
        assert_eq!(offset_to_position(src, 0), Position::new(0, 0));
        assert_eq!(offset_to_position(src, 2), Position::new(0, 2));
        assert_eq!(offset_to_position(src, 3), Position::new(1, 0));
        assert_eq!(offset_to_position(src, 4), Position::new(1, 1));
        // Clamped past the end.
        assert_eq!(offset_to_position(src, 99), Position::new(1, 2));
    }

    #[test]
    fn offset_to_position_uses_utf16_units() {
        // `√` is one UTF-16 code unit but three UTF-8 bytes; `𝕩` is two units.
        let src = "√4𝕩";
        assert_eq!(offset_to_position(src, 0), Position::new(0, 0));
        assert_eq!(offset_to_position(src, 3), Position::new(0, 1));
        assert_eq!(offset_to_position(src, 4), Position::new(0, 2));
        // After the astral char: 1 (√) + 1 (4) + 2 (𝕩) = 4 UTF-16 units.
        assert_eq!(offset_to_position(src, src.len()), Position::new(0, 4));
    }

    #[test]
    fn position_to_offset_round_trips() {
        let src = "?→A: √(B)◢";
        for (i, _) in src.char_indices() {
            let pos = offset_to_position(src, i);
            assert_eq!(position_to_offset(src, pos), Some(i), "offset {i}");
        }
    }

    #[test]
    fn position_to_offset_beyond_last_line_is_none() {
        assert_eq!(position_to_offset("A", Position::new(5, 0)), None);
    }

    #[test]
    fn valid_program_has_no_diagnostics() {
        assert!(diagnostics("?→A: A×2◢").is_empty());
        assert!(diagnostics("If A>0\nThen\n1◢\nIfEnd").is_empty());
    }

    #[test]
    fn lex_error_becomes_diagnostic() {
        let diags = diagnostics("A@B");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(diags[0].message.starts_with("Syntax ERROR"));
        // The `@` is at byte 1.
        assert_eq!(diags[0].range.start, Position::new(0, 1));
        assert_eq!(diags[0].range.end, Position::new(0, 2));
    }

    #[test]
    fn parse_error_becomes_diagnostic() {
        let diags = diagnostics("sin(");
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.contains("Syntax ERROR"),
            "unexpected message: {}",
            diags[0].message
        );
    }

    #[test]
    fn multiline_error_maps_to_correct_line() {
        let diags = diagnostics("1+1\n2+@");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].range.start.line, 1);
        assert_eq!(diags[0].range.start.character, 2);
    }

    #[test]
    fn completion_contains_required_entries() {
        let labels = labels();
        for expected in [
            "If",
            "Then",
            "Else",
            "IfEnd",
            "For",
            "To",
            "Step",
            "Next",
            "While",
            "WhileEnd",
            "Break",
            "Goto",
            "Lbl",
            "ClrMemory",
            "ClrStat",
            "sin(",
            "cos(",
            "tan(",
            "log(",
            "ln(",
            "√(",
            "∛(",
            "Abs(",
            "Pol(",
            "Rec(",
            "Rnd(",
            "A",
            "B",
            "C",
            "D",
            "X",
            "Y",
            "M",
            "Ans",
            "->",
            "=>",
            "disp",
            "<>",
            "<=",
            ">=",
        ] {
            assert!(labels.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn completion_is_deduplicated() {
        let labels = labels();
        let mut sorted = labels.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len(), "duplicate completion labels");
    }

    #[test]
    fn hover_on_variable() {
        let src = "A+1";
        let hover = hover(src, Position::new(0, 0)).expect("hover");
        match hover.contents {
            HoverContents::Markup(markup) => {
                assert!(markup.value.contains("Memory A"), "{}", markup.value);
            }
            other => panic!("unexpected hover contents: {other:?}"),
        }
        assert_eq!(
            hover.range,
            Some(Range::new(Position::new(0, 0), Position::new(0, 1)))
        );
    }

    #[test]
    fn hover_on_keyword() {
        let src = "Goto 1";
        let hover = hover(src, Position::new(0, 2)).expect("hover");
        match hover.contents {
            HoverContents::Markup(markup) => assert!(markup.value.contains("unconditional")),
            other => panic!("unexpected hover contents: {other:?}"),
        }
    }

    #[test]
    fn hover_on_whitespace_is_none() {
        assert!(hover("A   B", Position::new(0, 2)).is_none());
    }

    #[test]
    fn hover_ignores_lex_errors() {
        assert!(hover("A @ B", Position::new(0, 2)).is_none());
    }

    #[test]
    fn document_symbols_from_labels() {
        let symbols = document_symbols("Lbl 1: 1◢\nGoto 1\nLbl 2");
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "Lbl 1");
        assert_eq!(symbols[1].name, "Lbl 2");
        assert_eq!(symbols[0].range.start, Position::new(0, 0));
        assert_eq!(symbols[0].range.end, Position::new(0, 5));
    }

    #[test]
    fn document_symbols_empty_for_bad_source() {
        assert!(document_symbols("@@@").is_empty());
    }
}
