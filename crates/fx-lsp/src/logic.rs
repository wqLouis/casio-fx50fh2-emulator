//! Pure, testable language-server logic.
//!
//! Nothing in this module does I/O or depends on async.  It maps source text
//! to LSP structures: byte offsets to [`Position`]s, diagnostics produced by
//! the core lexer/parser, completion items, hover text and document symbols.

// The core enums are not `#[non_exhaustive]`, so the wildcard arms that exist
// purely for forward compatibility are unreachable today.  Keep them (so a new
// core variant does not break this crate) and silence the lint.
#![allow(unreachable_patterns)]

use std::path::Path;

use casio_fx50fh2::CalcError;
use casio_fx50fh2::compile;
use casio_fx50fh2::lexer::lex;
use casio_fx50fh2::token::{BinOp, ConstName, FuncName, Postfix, Token, TokenKind, VarName};
use fx_transpiler::ast::Stmt;
use fx_transpiler::error::TranspileError;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol,
    Documentation, Hover, HoverContents, InsertTextFormat, MarkupContent, MarkupKind,
    NumberOrString, Position, Range, SymbolKind,
};

// ---------------------------------------------------------------------------
// Languages
// ---------------------------------------------------------------------------

/// The source language a document is written in.
///
/// The language IDs are a fixed contract: editor configurations select a
/// server document by `fx` (PRGM, `.fx`) or `fxc` (C-like, `.fxc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// The calculator's own PRGM language.
    Prgm,
    /// The C-like language accepted by `fx-transpiler`.
    Fxc,
}

impl Language {
    /// Parse a client-supplied `languageId`.
    ///
    /// Recognises `fx`/`prgm` and `fxc`, case-insensitively; returns `None`
    /// for anything else so the caller can fall back to the file extension.
    pub fn from_id(id: &str) -> Option<Language> {
        if id.eq_ignore_ascii_case("fx") || id.eq_ignore_ascii_case("prgm") {
            Some(Language::Prgm)
        } else if id.eq_ignore_ascii_case("fxc") {
            Some(Language::Fxc)
        } else {
            None
        }
    }

    /// Infer the language from a path or URI, by extension.
    ///
    /// A case-insensitive `.fxc` suffix selects [`Language::Fxc`]; everything
    /// else is [`Language::Prgm`].
    pub fn from_path(path: &str) -> Language {
        if path.to_ascii_lowercase().ends_with(".fxc") {
            Language::Fxc
        } else {
            Language::Prgm
        }
    }

    /// The language ID advertised to clients.
    pub fn id(self) -> &'static str {
        match self {
            Language::Prgm => "fx",
            Language::Fxc => "fxc",
        }
    }

    /// A human-readable label for logs and documentation.
    pub fn label(self) -> &'static str {
        match self {
            Language::Prgm => "PRGM",
            Language::Fxc => "C-like",
        }
    }
}

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

/// Convert a 1-based `line`/`column` (column counted in `char`s, matching
/// [`TranspileError`]) into the [`Range`] covering that character.
///
/// Out-of-range input is clamped rather than panicking: a line past the end of
/// the document maps to a zero-width range at the end of the text, and a
/// column past the end of its line to the end of that line.  Line and column
/// `0` are treated as `1`.
pub fn range_from_line_col(source: &str, line: usize, column: usize) -> Range {
    let target_line = line.saturating_sub(1);
    let target_col = column.saturating_sub(1);

    // Move to the byte offset at the start of the requested line.
    let mut line_start = 0usize;
    let mut current_line = 0usize;
    while current_line < target_line {
        match source[line_start..].find('\n') {
            Some(relative) => {
                line_start += relative + 1;
                current_line += 1;
            }
            None => {
                // The requested line does not exist: zero-width range at EOF.
                let end = offset_to_position(source, source.len());
                return Range::new(end, end);
            }
        }
    }

    let line_end = source[line_start..]
        .find('\n')
        .map(|relative| line_start + relative)
        .unwrap_or(source.len());
    let line_text = &source[line_start..line_end];

    let start = line_start + char_col_offset(line_text, target_col);
    let end = if target_col >= line_text.chars().count() {
        start
    } else {
        line_start + char_col_offset(line_text, target_col + 1)
    };
    Range::new(
        offset_to_position(source, start),
        offset_to_position(source, end),
    )
}

/// The byte offset of the `char_col`-th `char` (0-based) within `line`.
///
/// Clamps to the line's length when `char_col` is past its final character.
fn char_col_offset(line: &str, char_col: usize) -> usize {
    line.char_indices()
        .nth(char_col)
        .map(|(offset, _)| offset)
        .unwrap_or(line.len())
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
/// Both `Syntax` and `Mode` errors carry a position; errors without one (for
/// example a mode violation produced by the checker, which has no single
/// offending byte) fall back to the start of the document.
pub fn error_position(err: &CalcError) -> Option<usize> {
    err.pos()
}

/// Build an LSP diagnostic from a core error.
pub fn diagnostic(source: &str, err: &CalcError) -> Diagnostic {
    let range = match error_position(err) {
        Some(offset) => range_for_offset(source, offset),
        // No position: point at the start of the document (0..0).
        None => Range::new(Position::new(0, 0), Position::new(0, 0)),
    };
    Diagnostic {
        range,
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

/// Lex, parse and mode-check `source` for `language`, returning diagnostics.
///
/// `Prgm` delegates to [`casio_fx50fh2::compile`], so a `#mode` header is
/// honoured and using complex constructs outside CMPLX, statistics outside
/// SD/REG, or base-n outside BASE is reported as a `Mode ERROR` alongside
/// lexical and syntactic errors.  A lex error short-circuits before parsing
/// (inside `compile`).
///
/// `Fxc` delegates to [`fx_transpiler::transpile_with_base`], which expands
/// `#include` directives relative to `base_dir`.  A missing include therefore
/// surfaces as a diagnostic rather than being silently ignored.  When
/// `base_dir` is `None` or does not exist, the current directory is used so a
/// broken include can never panic the server.  Purely runtime errors (Math
/// ERROR etc.) are not reported by the language server.
pub fn diagnostics(source: &str, language: Language, base_dir: Option<&Path>) -> Vec<Diagnostic> {
    match language {
        Language::Prgm => match compile(source) {
            Ok(_) => Vec::new(),
            Err(err) => vec![diagnostic(source, &err)],
        },
        Language::Fxc => {
            let base = match base_dir {
                Some(dir) if dir.exists() => dir,
                _ => Path::new("."),
            };
            match fx_transpiler::transpile_with_base(
                source,
                fx_transpiler::Options::default(),
                base,
            ) {
                Ok(_) => Vec::new(),
                Err(err) => vec![transpile_diagnostic(source, &err)],
            }
        }
    }
}

/// Build an LSP diagnostic from a `.fxc` transpiler error.
///
/// The range prefers the error's 1-based line/column and falls back to its
/// byte offset when those are unknown.  The code distinguishes an error in the
/// document itself (`Transpile ERROR`) from one that involves another file —
/// an included fragment, or an `#include` that could not be resolved
/// (`Include ERROR`).  The offending file is named in the message either way.
pub fn transpile_diagnostic(source: &str, err: &TranspileError) -> Diagnostic {
    let range = if err.line == 0 || err.column == 0 {
        range_for_offset(source, err.offset)
    } else {
        range_from_line_col(source, err.line, err.column)
    };
    let code = if is_include_error(err) {
        "Include ERROR"
    } else {
        "Transpile ERROR"
    };
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(code.to_string())),
        code_description: None,
        source: Some("fx-50FH II".to_string()),
        message: err.to_string(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Whether `err` concerns an `#include` or another file rather than the
/// document being edited.
///
/// [`fx_transpiler::transpile_with_base`] transpiles anonymous text, so a
/// compile error in the document carries no `file`, while an error inside an
/// included fragment names that fragment.  An `#include` that cannot be
/// resolved at the root is reported without a file too, so its message is the
/// only signal.
fn is_include_error(err: &TranspileError) -> bool {
    err.file.is_some()
        || err.message.starts_with("cannot include")
        || err.message.starts_with("circular `#include`")
        || err.message.starts_with("`#include`")
        || err.message.contains("included fragment")
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

/// Every completion item offered for `language`.
///
/// Both lists are context-free and de-duplicated by label.  The PRGM list is
/// the calculator's tiny keyboard vocabulary; the `.fxc` list is the C-like
/// grammar's keywords, built-ins, constants and operators.
pub fn completion_items(language: Language) -> Vec<CompletionItem> {
    match language {
        Language::Prgm => prgm_completion_items(),
        Language::Fxc => fxc_completion_items(),
    }
}

/// The PRGM completion list.
///
/// The list is deliberately context-free: the calculator keyboard is tiny, so
/// offering the whole vocabulary is cheap and predictable.
fn prgm_completion_items() -> Vec<CompletionItem> {
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

    // -- mode directives ----------------------------------------------------
    // A bare `#mode` inserts the directive and a trailing space; the five
    // concrete items complete it to a valid mode name.
    push(
        &mut items,
        CompletionItem {
            label: "#mode".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("operating mode directive".to_string()),
            documentation: Some(Documentation::String(
                "Declare the calculator operating mode: COMP, CMPLX, BASE, SD or REG".to_string(),
            )),
            insert_text: Some("#mode ".to_string()),
            ..Default::default()
        },
    );
    for mode in ["COMP", "CMPLX", "BASE", "SD", "REG"] {
        push(
            &mut items,
            simple(
                &format!("#mode {mode}"),
                CompletionItemKind::KEYWORD,
                "operating mode",
            ),
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
    // The 40 scientific constants, offered under the ASCII name so that what
    // is inserted is always typeable.
    for c in &casio_fx50fh2::CONSTANTS {
        push(
            &mut items,
            simple(
                c.name,
                CompletionItemKind::CONSTANT,
                &format!(
                    "{} (CONST {:02}, displays as `{}`)",
                    c.description, c.code, c.symbol
                ),
            ),
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

/// The `.fxc` completion list.
fn fxc_completion_items() -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // -- keywords -----------------------------------------------------------
    let keywords = [
        ("let", "declare a variable (uses one of A B C D X Y M)"),
        (
            "const",
            "declare a compile-time constant (inlined; uses no memory)",
        ),
        (
            "free",
            "release a variable's memory so a later variable can reuse it",
        ),
        (
            "unsafe_free",
            "release a memory without the `goto`/`label` safety check",
        ),
        ("if", "conditional execution"),
        ("else", "alternative `if` body"),
        ("while", "conditional loop"),
        ("for", "counted loop"),
        ("break", "exit the innermost loop"),
        ("goto", "unconditional jump to a label"),
        ("label", "jump label"),
        ("print", "display a value"),
    ];
    for (label, detail) in keywords {
        push(
            &mut items,
            simple(label, CompletionItemKind::KEYWORD, detail),
        );
    }

    // -- whole-program directives ------------------------------------------
    push(
        &mut items,
        CompletionItem {
            label: "#data".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("compile-time data table".to_string()),
            documentation: Some(Documentation::String(
                "Read a JSON value while transpiling, as in\n\n    #data config = { \"n\": 3 };\n\n\
                 A top-level string is a file path: `#data v = \"values.json\";`. The values \
                 become literals, so they use none of the seven memories."
                    .to_string(),
            )),
            insert_text: Some("#data ".to_string()),
            ..Default::default()
        },
    );
    push(
        &mut items,
        CompletionItem {
            label: "#tests".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("embedded test cases".to_string()),
            documentation: Some(Documentation::String(
                "Cases run by `fx50 test`, as in\n\n    #tests = [\n      { \"name\": \"one\", \"output\": [\"1\"] }\n    ];"
                    .to_string(),
            )),
            insert_text: Some("#tests = [\n  { \"name\": \"\" }\n];".to_string()),
            ..Default::default()
        },
    );

    // -- input and built-in functions --------------------------------------
    push(
        &mut items,
        function(
            "input()",
            "input()",
            "read a number from the user (assignment right-hand side only)",
        ),
    );
    for builtin in fx_transpiler::builtins::BUILTINS {
        let detail = if builtin.min_args == builtin.max_args {
            format!("built-in: {} argument", builtin.min_args)
        } else {
            format!(
                "built-in: {} to {} arguments (e.g. `log(x)` or `log(base, x)`)",
                builtin.min_args, builtin.max_args
            )
        };
        push(
            &mut items,
            function(
                &format!("{}(", builtin.name),
                &format!("{}($0)", builtin.name),
                &detail,
            ),
        );
    }

    // -- constants ----------------------------------------------------------
    push(
        &mut items,
        simple("pi", CompletionItemKind::CONSTANT, "the constant pi"),
    );
    push(
        &mut items,
        simple("e", CompletionItemKind::CONSTANT, "Euler's number"),
    );
    for constant in &fx_transpiler::constants::CONSTANTS {
        push(
            &mut items,
            simple(
                &format!("phys.{}", constant.name),
                CompletionItemKind::CONSTANT,
                &format!(
                    "{} (CONST {:02}, displays as `{}`)",
                    constant.description, constant.code, constant.symbol
                ),
            ),
        );
    }
    push(
        &mut items,
        CompletionItem {
            label: "phys.".to_string(),
            kind: Some(CompletionItemKind::CONSTANT),
            detail: Some("scientific-constant namespace".to_string()),
            documentation: Some(Documentation::String(
                "Scientific constants are reached as `phys.NAME`, for example \
                 `phys.h` or `phys.hbar`."
                    .to_string(),
            )),
            ..Default::default()
        },
    );

    // -- operators ----------------------------------------------------------
    let operators = [
        ("==", "equality test"),
        ("!=", "inequality test"),
        ("<=", "less than or equal"),
        (">=", "greater than or equal"),
        ("<", "less than"),
        (">", "greater than"),
        ("+", "addition"),
        ("-", "subtraction"),
        ("*", "multiplication"),
        ("/", "division"),
        ("^", "power"),
        ("**", "power (alias for `^`)"),
        ("=", "assignment"),
        (";", "statement terminator"),
    ];
    for (label, detail) in operators {
        push(
            &mut items,
            simple(label, CompletionItemKind::OPERATOR, detail),
        );
    }

    // -- directives ---------------------------------------------------------
    push(
        &mut items,
        CompletionItem {
            label: "#mode".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("operating mode directive".to_string()),
            documentation: Some(Documentation::String(
                "Declare the operating mode: COMP, CMPLX, BASE, SD or REG".to_string(),
            )),
            insert_text: Some("#mode ".to_string()),
            ..Default::default()
        },
    );
    for mode in ["COMP", "CMPLX", "BASE", "SD", "REG"] {
        push(
            &mut items,
            simple(
                &format!("#mode {mode}"),
                CompletionItemKind::KEYWORD,
                "operating mode",
            ),
        );
    }
    push(
        &mut items,
        CompletionItem {
            label: "#include".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("inline another `.fxc` file".to_string()),
            documentation: Some(Documentation::String(
                "Include another `.fxc` fragment, resolved relative to this file".to_string(),
            )),
            insert_text: Some("#include \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
    );

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

/// Hover information for the word or token under `position`.
pub fn hover(source: &str, position: Position, language: Language) -> Option<Hover> {
    match language {
        Language::Prgm => prgm_hover(source, position),
        Language::Fxc => fxc_hover(source, position),
    }
}

/// PRGM hover: describe the token under `position`.
fn prgm_hover(source: &str, position: Position) -> Option<Hover> {
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
        ConstName::Physical(code) => casio_fx50fh2::constants::by_code(*code)
            .map_or("Scientific constant", |p| p.description),
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
        TokenKind::ModeDirective(mode) => format!(
            "**`#mode`** — operating mode directive\n\n\
             Declares the calculator mode for the program (current: `{mode}`).\n\n\
             - `COMP` — general computation, real numbers only (the default)\n\
             - `CMPLX` — complex numbers (`i`, `∠`, `arg`, `Conjg`)\n\
             - `BASE` — base-n integers (`Dec`/`Hex`/`Bin`/`Oct`, bitwise operators)\n\
             - `SD` — single-variable statistics\n\
             - `REG` — paired-variable statistics and regression\n\n\
             Complex constructs need `CMPLX`, statistics need `SD` or `REG`, and \
             base-n needs `BASE`; a program with no directive runs in `COMP`."
        ),
        TokenKind::Var(v) => {
            format!("**`{}`** — variable\n\n{}", v.name(), var_description(v))
        }
        TokenKind::Const(c) => match c {
            ConstName::Physical(code) => match casio_fx50fh2::constants::by_code(*code) {
                Some(p) => format!(
                    "**`{}`** — scientific constant\n\n{} (`{}`, CONST {:02})\n\n\
                     Value: {:e} {}",
                    p.symbol, p.description, p.name, p.code, p.value, p.unit
                ),
                None => "**?** — scientific constant".to_string(),
            },
            _ => {
                let name = match c {
                    ConstName::Pi => "π",
                    ConstName::E => "e",
                    ConstName::I => "i",
                    ConstName::Physical(_) => unreachable!(),
                };
                format!("**`{name}`** — constant\n\n{}", const_description(c))
            }
        },
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

/// The character at `offset`, if it is a valid boundary.
fn char_at(source: &str, offset: usize) -> Option<char> {
    source.get(offset..)?.chars().next()
}

/// The start of the character immediately before `offset`.
fn prev_char_start(source: &str, offset: usize) -> Option<usize> {
    let offset = offset.min(source.len());
    if offset == 0 {
        return None;
    }
    let mut previous = offset - 1;
    while previous > 0 && !source.is_char_boundary(previous) {
        previous -= 1;
    }
    Some(previous)
}

/// Whether `c` can be part of a `.fxc` identifier or constant symbol.
///
/// The `.fxc` lexer accepts ASCII names plus, after `phys.`, Unicode letters
/// and the `∞` in `R∞`.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || (c.is_alphabetic() && !c.is_ascii()) || c == '∞'
}

/// The byte span of the identifier-like word containing `offset`.
///
/// Mirrors the PRGM [`token_at`] semantics: a position one past the end of a
/// word is *not* inside it.  Returns `None` when the cursor is not on a word
/// character.
fn word_span(source: &str, offset: usize) -> Option<(usize, usize)> {
    let c = char_at(source, offset)?;
    if !is_word_char(c) {
        return None;
    }

    let mut start = offset;
    while let Some(previous) = prev_char_start(source, start) {
        match char_at(source, previous) {
            Some(c) if is_word_char(c) => start = previous,
            _ => break,
        }
    }

    let mut end = next_char_boundary(source, offset);
    while end < source.len() {
        match char_at(source, end) {
            Some(c) if is_word_char(c) => end = next_char_boundary(source, end),
            _ => break,
        }
    }
    Some((start, end))
}

/// Whether the word starting at `word_start` is the `NAME` of a `phys.NAME`.
///
/// Spaces around the dot are ignored so `phys. h` and `phys . h` are both
/// recognised, matching the parser (which skips whitespace between tokens).
fn preceded_by_phys(source: &str, word_start: usize) -> bool {
    let before = source[..word_start].trim_end_matches([' ', '\t']);
    let Some(prefix) = before.strip_suffix('.') else {
        return false;
    };
    let prefix = prefix.trim_end_matches([' ', '\t']);
    let Some(prefix) = prefix.strip_suffix("phys") else {
        return false;
    };
    !prefix.chars().next_back().is_some_and(is_word_char)
}

/// Hover for `.fxc`.
///
/// The description is found without the transpiler's lexer, so it still works
/// while the document is incomplete.  Crucially, a bare name is described as a
/// variable: only a name written `phys.NAME` is a scientific constant.
fn fxc_hover(source: &str, position: Position) -> Option<Hover> {
    let offset = position_to_offset(source, position)?;
    let (start, end) = word_span(source, offset)?;
    let word = &source[start..end];
    let value = fxc_description(source, word, start);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(Range::new(
            offset_to_position(source, start),
            offset_to_position(source, end),
        )),
    })
}

/// Markdown description for a `.fxc` word.
fn fxc_description(source: &str, word: &str, start: usize) -> String {
    if preceded_by_phys(source, start) {
        return match fx_transpiler::constants::lookup(word) {
            Some(constant) => format!(
                "**`phys.{word}`** — scientific constant\n\n{} (ASCII `{}`, displays as `{}`, CONST {:02})",
                constant.description, constant.name, constant.symbol, constant.code
            ),
            None => format!(
                "**`phys.{word}`** — unknown scientific constant\n\nNo built-in constant is named `{word}`."
            ),
        };
    }

    match word {
        "phys" => "**`phys`** — scientific-constant namespace\n\nWrite `phys.NAME`, for example `phys.h` (Planck constant) or `phys.hbar` (reduced Planck constant).".to_string(),
        "let" => "**`let`** — declare a variable".to_string(),
        "if" => "**`if`** — conditional execution".to_string(),
        "else" => "**`else`** — alternative `if` body".to_string(),
        "while" => "**`while`** — conditional loop".to_string(),
        "for" => "**`for`** — counted loop".to_string(),
        "break" => "**`break`** — exit the innermost loop".to_string(),
        "goto" => "**`goto`** — unconditional jump to a label".to_string(),
        "label" => "**`label`** — jump label".to_string(),
        "print" => "**`print`** — display a value".to_string(),
        "pi" => "**`pi`** — the constant π (3.14159…)".to_string(),
        "e" => "**`e`** — Euler's number (2.71828…)".to_string(),
        "input" => "**`input()`** — read a number from the user\n\nOnly valid as the whole right-hand side of an assignment.".to_string(),
        _ => {
            if let Some(builtin) = fx_transpiler::builtins::lookup(word) {
                let arity = if builtin.min_args == builtin.max_args {
                    builtin.min_args.to_string()
                } else {
                    format!("{} to {}", builtin.min_args, builtin.max_args)
                };
                return format!(
                    "**`{word}`** — built-in function\n\nTakes {arity} argument(s); emits `{}`.",
                    builtin.glyph
                );
            }
            let hint = if fx_transpiler::constants::lookup(word).is_some() {
                format!("\n\nIf you meant the scientific constant, write `phys.{word}`.")
            } else {
                String::new()
            };
            format!(
                "**`{word}`** — variable\n\nIn `.fxc` a bare name is an ordinary variable, assigned to one of the seven memories.{hint}"
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Document symbols
// ---------------------------------------------------------------------------

/// Document symbols for `language`: `Lbl` markers for PRGM, labels and
/// variable declarations for `.fxc`.
pub fn document_symbols(source: &str, language: Language) -> Vec<DocumentSymbol> {
    match language {
        Language::Prgm => prgm_document_symbols(source),
        Language::Fxc => fxc_document_symbols(source),
    }
}

/// One symbol per `Lbl` marker in the program.
///
/// Tokens are scanned directly rather than walking the AST so that symbols
/// still work while the rest of the document has a syntax error.
#[allow(deprecated)]
fn prgm_document_symbols(source: &str) -> Vec<DocumentSymbol> {
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

/// `.fxc` document symbols: every `label N` and declared variable.
///
/// A document that does not lex or parse yields no symbols rather than an
/// error.  Names are de-duplicated (the first occurrence wins) and the result
/// is sorted by source position.
fn fxc_document_symbols(source: &str) -> Vec<DocumentSymbol> {
    let Ok(tokens) = fx_transpiler::lexer::lex(source) else {
        return Vec::new();
    };
    let Ok(program) = fx_transpiler::parser::parse(&tokens, source) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();
    let mut seen = Vec::new();
    collect_fxc_symbols(&program, source, &mut symbols, &mut seen);
    symbols.sort_by(|a, b| {
        (a.range.start.line, a.range.start.character)
            .cmp(&(b.range.start.line, b.range.start.character))
    });
    symbols
}

/// Walk `statements`, collecting `label N` and variable declarations.
fn collect_fxc_symbols(
    statements: &[Stmt],
    source: &str,
    symbols: &mut Vec<DocumentSymbol>,
    seen: &mut Vec<String>,
) {
    for statement in statements {
        match statement {
            Stmt::Label(number, pos) => {
                let name = format!("label {number}");
                if !seen.contains(&name) {
                    seen.push(name.clone());
                    symbols.push(fxc_symbol(
                        name,
                        SymbolKind::FUNCTION,
                        "jump label",
                        source,
                        *pos,
                    ));
                }
            }
            Stmt::Let { name, pos, .. } => {
                if !seen.contains(name) {
                    seen.push(name.clone());
                    symbols.push(fxc_symbol(
                        name.clone(),
                        SymbolKind::VARIABLE,
                        "variable",
                        source,
                        *pos,
                    ));
                }
            }
            Stmt::For(for_stmt) => {
                if !seen.contains(&for_stmt.init_name) {
                    seen.push(for_stmt.init_name.clone());
                    symbols.push(fxc_symbol(
                        for_stmt.init_name.clone(),
                        SymbolKind::VARIABLE,
                        "loop variable",
                        source,
                        for_stmt.pos,
                    ));
                }
                collect_fxc_symbols(&for_stmt.body, source, symbols, seen);
            }
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_fxc_symbols(then_body, source, symbols, seen);
                collect_fxc_symbols(else_body, source, symbols, seen);
            }
            Stmt::While { body, .. } => collect_fxc_symbols(body, source, symbols, seen),
            Stmt::Block(body) => collect_fxc_symbols(body, source, symbols, seen),
            _ => {}
        }
    }
}

/// Build a `.fxc` [`DocumentSymbol`] whose range is the declaration's start.
#[allow(deprecated)]
fn fxc_symbol(
    name: String,
    kind: SymbolKind,
    detail: &str,
    source: &str,
    offset: usize,
) -> DocumentSymbol {
    let range = range_for_offset(source, offset);
    DocumentSymbol {
        name,
        detail: Some(detail.to_string()),
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // The module entry points now take a `Language`; bind the PRGM variants
    // locally so the existing PRGM tests stay unchanged.
    fn diagnostics(source: &str) -> Vec<Diagnostic> {
        super::diagnostics(source, Language::Prgm, None)
    }

    fn hover(source: &str, position: Position) -> Option<Hover> {
        super::hover(source, position, Language::Prgm)
    }

    fn document_symbols(source: &str) -> Vec<DocumentSymbol> {
        super::document_symbols(source, Language::Prgm)
    }

    fn completion_items() -> Vec<CompletionItem> {
        super::completion_items(Language::Prgm)
    }

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
    fn completion_offers_the_scientific_constants() {
        let labels = labels();
        for expected in ["h", "hbar", "mp", "Rinf", "eq", "atm", "C0"] {
            assert!(
                labels.iter().any(|l| l == expected),
                "`{expected}` missing from completions"
            );
        }
        // All 40 are offered, and `h` is the Planck constant's own entry
        // rather than a variable.
        let constant = completion_items()
            .into_iter()
            .find(|item| item.label == "h")
            .expect("h");
        assert_eq!(constant.kind, Some(CompletionItemKind::CONSTANT));
        assert!(
            constant.detail.unwrap_or_default().contains("Planck"),
            "detail should name the constant"
        );
    }

    #[test]
    fn hover_on_a_scientific_constant() {
        // `h` is at byte 0, `hbar` at byte 4.
        let src = "h+hbar";
        match hover(src, Position::new(0, 0)).expect("hover").contents {
            HoverContents::Markup(markup) => {
                assert!(markup.value.contains("Planck constant"), "{}", markup.value);
                assert!(markup.value.contains("CONST 06"), "{}", markup.value);
            }
            other => panic!("unexpected hover contents: {other:?}"),
        }
        match hover(src, Position::new(0, 4)).expect("hover").contents {
            HoverContents::Markup(markup) => {
                assert!(
                    markup.value.contains("reduced Planck constant"),
                    "{}",
                    markup.value
                );
                assert!(markup.value.contains("CONST 09"), "{}", markup.value);
            }
            other => panic!("unexpected hover contents: {other:?}"),
        }
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

    fn diagnostic_codes(source: &str) -> Vec<String> {
        diagnostics(source)
            .into_iter()
            .map(|diag| match diag.code {
                Some(NumberOrString::String(code)) => code,
                other => panic!("unexpected diagnostic code: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn complex_without_mode_header_is_mode_error() {
        let diags = diagnostics("3+4i");
        assert_eq!(diagnostic_codes("3+4i"), vec!["Mode ERROR".to_string()]);
        // The checker's mode errors carry no byte offset, so the diagnostic
        // falls back to the start of the document (0..0).
        assert_eq!(diags[0].range.start, Position::new(0, 0));
        assert_eq!(diags[0].range.end, Position::new(0, 0));
    }

    #[test]
    fn complex_with_mode_header_is_accepted() {
        assert!(diagnostics("#mode CMPLX\n3+4i").is_empty());
    }

    #[test]
    fn base_literal_needs_base_mode() {
        assert_eq!(diagnostic_codes("Hex: FFh"), vec!["Mode ERROR".to_string()]);
        assert!(diagnostics("#mode BASE\nHex: FFh").is_empty());
    }

    #[test]
    fn completion_contains_mode_directives() {
        let labels = labels();
        for expected in [
            "#mode",
            "#mode COMP",
            "#mode CMPLX",
            "#mode BASE",
            "#mode SD",
            "#mode REG",
        ] {
            assert!(labels.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn hover_on_mode_directive() {
        let src = "#mode CMPLX\n3+4i";
        let hover = hover(src, Position::new(0, 3)).expect("hover");
        match hover.contents {
            HoverContents::Markup(markup) => {
                assert!(markup.value.contains("operating mode"), "{}", markup.value);
                assert!(markup.value.contains("COMP"), "{}", markup.value);
                assert!(markup.value.contains("CMPLX"), "{}", markup.value);
                assert!(markup.value.contains("BASE"), "{}", markup.value);
                assert!(markup.value.contains("SD"), "{}", markup.value);
                assert!(markup.value.contains("REG"), "{}", markup.value);
            }
            other => panic!("unexpected hover contents: {other:?}"),
        }
    }

    // -- Language -----------------------------------------------------------

    #[test]
    fn language_from_id() {
        assert_eq!(Language::from_id("fx"), Some(Language::Prgm));
        assert_eq!(Language::from_id("prgm"), Some(Language::Prgm));
        assert_eq!(Language::from_id("fxc"), Some(Language::Fxc));
        assert_eq!(Language::from_id("FXC"), Some(Language::Fxc));
        assert_eq!(Language::from_id("unknown"), None);
    }

    #[test]
    fn language_from_path() {
        assert_eq!(Language::from_path("main.fxc"), Language::Fxc);
        assert_eq!(Language::from_path("MAIN.FXC"), Language::Fxc);
        assert_eq!(Language::from_path("main.fx"), Language::Prgm);
        assert_eq!(Language::from_path("main.FX"), Language::Prgm);
        assert_eq!(Language::from_path("main.txt"), Language::Prgm);
        assert_eq!(Language::from_path("file:///x/y.fxc"), Language::Fxc);
    }

    #[test]
    fn language_ids_and_labels() {
        assert_eq!(Language::Prgm.id(), "fx");
        assert_eq!(Language::Fxc.id(), "fxc");
        assert_eq!(Language::Prgm.label(), "PRGM");
        assert_eq!(Language::Fxc.label(), "C-like");
    }

    // -- range_from_line_col ------------------------------------------------

    #[test]
    fn range_from_line_col_basics() {
        let src = "let a = 1;\nlet b = 2;\n";
        assert_eq!(
            range_from_line_col(src, 1, 1),
            Range::new(Position::new(0, 0), Position::new(0, 1))
        );
        assert_eq!(
            range_from_line_col(src, 2, 5),
            Range::new(Position::new(1, 4), Position::new(1, 5))
        );
    }

    #[test]
    fn range_from_line_col_is_robust_out_of_range() {
        let src = "abc";
        // Line 0 and column 0 clamp to the first character.
        assert_eq!(
            range_from_line_col(src, 0, 0),
            Range::new(Position::new(0, 0), Position::new(0, 1))
        );
        // A line past EOF is a zero-width range at the end of the document.
        let end = Position::new(0, 3);
        assert_eq!(range_from_line_col(src, 99, 1), Range::new(end, end));
        // A column past the end of the line clamps to the line end.
        assert_eq!(range_from_line_col(src, 1, 99), Range::new(end, end));
    }

    #[test]
    fn range_from_line_col_uses_utf16_units() {
        let src = "√4 = 2";
        // The `4` is the second `char`; its LSP column is 1 (UTF-16).
        assert_eq!(
            range_from_line_col(src, 1, 2),
            Range::new(Position::new(0, 1), Position::new(0, 2))
        );
    }

    // -- .fxc smoke tests ---------------------------------------------------

    #[test]
    fn fxc_valid_program_has_no_diagnostics() {
        assert!(super::diagnostics("let a = 1; print(a);", Language::Fxc, None).is_empty());
    }

    #[test]
    fn fxc_syntax_error_is_a_transpile_error() {
        let diags = super::diagnostics("let a = ;", Language::Fxc, None);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].code,
            Some(NumberOrString::String("Transpile ERROR".to_string()))
        );
    }

    #[test]
    fn fxc_base_mode_rejects_builtin() {
        let diags = super::diagnostics("#mode BASE\nprint(sqrt(4));", Language::Fxc, None);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn fxc_completion_has_no_prgm_labels() {
        let labels: Vec<String> = super::completion_items(Language::Fxc)
            .into_iter()
            .map(|item| item.label)
            .collect();
        assert!(labels.contains(&"sqrt(".to_string()), "missing sqrt(");
        assert!(!labels.iter().any(|l| l == "IfEnd" || l == "Lbl"));
    }

    #[test]
    fn fxc_hover_variable_and_constant() {
        let src = "let hbar = phys.h;";
        let bare = super::hover(src, Position::new(0, 5), Language::Fxc).expect("bare hbar");
        match bare.contents {
            HoverContents::Markup(markup) => {
                assert!(markup.value.contains("variable"), "{}", markup.value);
                assert!(markup.value.contains("phys.hbar"), "{}", markup.value);
            }
            other => panic!("unexpected hover contents: {other:?}"),
        }
        let constant = super::hover(src, Position::new(0, 16), Language::Fxc).expect("phys.h");
        match constant.contents {
            HoverContents::Markup(markup) => {
                assert!(markup.value.contains("Planck constant"), "{}", markup.value);
            }
            other => panic!("unexpected hover contents: {other:?}"),
        }
    }

    #[test]
    fn fxc_document_symbols_labels_and_variables() {
        let symbols = super::document_symbols(
            "label 1;\nlet a = 1;\nfor (let i = 0; i < 3; i = i + 1) { print(i); }",
            Language::Fxc,
        );
        let names: Vec<String> = symbols.iter().map(|symbol| symbol.name.clone()).collect();
        assert!(names.contains(&"label 1".to_string()));
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"i".to_string()));
    }

    #[test]
    fn fxc_document_symbols_empty_for_malformed_source() {
        assert!(super::document_symbols("let a = ;", Language::Fxc).is_empty());
    }
}
