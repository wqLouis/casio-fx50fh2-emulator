//! A small, dependency-free JSON parser and writer.
//!
//! `fx-transpiler` must build with `--no-default-features` and **zero**
//! dependencies (ADR 0006), but `#data` is a language feature: it cannot be
//! hidden behind a cargo feature without the language changing shape with the
//! build configuration. So JSON is implemented here rather than pulled from
//! `serde_json`, and this module is the single JSON implementation in the
//! crate — [`crate::testing`] uses it too, so there is no second parser that
//! could disagree.
//!
//! The parser is deliberately strict, because its input is compiled into a
//! calculator program and silent coercion is worse than a clear error:
//!
//! * exactly RFC 8259 syntax — no comments, no trailing commas;
//! * **duplicate object keys are an error** (most parsers keep the last, which
//!   turns a typo into a silently changed value);
//! * numbers must fit an `f64`; `1e400` is out of range rather than `inf`;
//! * lone UTF-16 surrogates in `\u` escapes are rejected;
//! * nesting is capped at [`MAX_DEPTH`] so adversarial input cannot overflow
//!   the stack.
//!
//! Numbers are `f64`, which is what the calculator computes with. An integer
//! beyond 2^53 therefore loses precision on parse (`9007199254740993` becomes
//! `9007199254740992`), which is the same rounding the calculator would apply;
//! values that must stay exact should be written as strings and used as
//! strings, or kept within the 15 significant digits the machine stores.
//!
//! ```
//! # use fx_transpiler::json::{parse, Json};
//! let value = parse(r#"{"n": 3, "xs": [1, 2]}"#).unwrap();
//! assert_eq!(value.get("n").and_then(Json::as_f64), Some(3.0));
//! assert_eq!(value.get("xs").and_then(|xs| xs.index(0)).and_then(Json::as_f64), Some(1.0));
//! ```

use std::fmt;

/// Maximum nesting depth accepted by [`parse`].
pub const MAX_DEPTH: usize = 128;

/// A parsed JSON value.
///
/// Objects keep their entries in document order and never contain duplicate
/// keys, which makes [`Json::as_object`] a faithful representation of what was
/// written.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// A short human-readable name for the value's type, for diagnostics.
    pub fn type_name(&self) -> &'static str {
        match self {
            Json::Null => "null",
            Json::Bool(_) => "boolean",
            Json::Number(_) => "number",
            Json::String(_) => "string",
            Json::Array(_) => "array",
            Json::Object(_) => "object",
        }
    }

    /// Look up an object member. Returns `None` for non-objects.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Look up an array element by index. Returns `None` for non-arrays and
    /// out-of-range indices.
    pub fn index(&self, index: usize) -> Option<&Json> {
        match self {
            Json::Array(items) => items.get(index),
            _ => None,
        }
    }

    /// The value as an `f64`, if it is a number.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// The value as a number: booleans widen to `1`/`0`, matching the
    /// calculator, which has no boolean type.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Json::Number(value) => Some(*value),
            Json::Bool(true) => Some(1.0),
            Json::Bool(false) => Some(0.0),
            _ => None,
        }
    }

    /// The value as a boolean, if it is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// The value as a string, if it is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    /// The value's elements, if it is an array.
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    /// The value's members, if it is an object.
    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Object(entries) => Some(entries),
            _ => None,
        }
    }

    /// Build an object from key/value pairs.
    pub fn object<K, I>(entries: I) -> Json
    where
        K: Into<String>,
        I: IntoIterator<Item = (K, Json)>,
    {
        Json::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    /// Build an array.
    pub fn array<I: IntoIterator<Item = Json>>(items: I) -> Json {
        Json::Array(items.into_iter().collect())
    }

    /// A JSON string value.
    pub fn string(text: impl Into<String>) -> Json {
        Json::String(text.into())
    }
}

/// Why a JSON document could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub message: String,
    /// Byte offset into the input.
    pub offset: usize,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column, counted in `char`s.
    pub column: usize,
}

impl JsonError {
    fn at(source: &str, message: impl Into<String>, offset: usize) -> Self {
        let (line, column) = line_col(source, offset);
        JsonError {
            message: message.into(),
            offset,
            line,
            column,
        }
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid JSON at line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for JsonError {}

/// Map a byte offset to a 1-based `(line, column)` pair, counting `char`s.
fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let limit = offset.min(source.len());
    let mut line = 1usize;
    let mut column = 1usize;
    for (index, ch) in source.char_indices() {
        if index >= limit {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

/// Parse a complete JSON document.
///
/// Leading and trailing whitespace is allowed; anything else after the value
/// is an error. A leading UTF-8 BOM is skipped.
pub fn parse(text: &str) -> Result<Json, JsonError> {
    let mut parser = Parser::new(text);
    let value = parser.value(0)?;
    parser.skip_whitespace();
    if let Some(ch) = parser.peek() {
        return Err(parser.error(format!("unexpected trailing character `{ch}`")));
    }
    Ok(value)
}

/// Parse one JSON value from the start of `text`, returning it together with
/// the byte offset just past it.
///
/// This is what the `#data` directive needs: the value ends where the JSON
/// grammar says it does, and the caller is responsible for whatever follows
/// (the `;` that terminates the directive). Trailing text is not an error, so
/// `parse_prefix("1;")` yields `(1, 1)`.
pub fn parse_prefix(text: &str) -> Result<(Json, usize), JsonError> {
    let mut parser = Parser::new(text);
    parser.skip_whitespace();
    let value = parser.value(0)?;
    Ok((value, parser.byte))
}

struct Parser<'a> {
    source: &'a str,
    chars: Vec<char>,
    i: usize,
    byte: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        let mut parser = Parser {
            source,
            chars: source.chars().collect(),
            i: 0,
            byte: 0,
        };
        // A BOM is not part of the document; skipping it here (rather than
        // trimming the string) keeps byte offsets aligned with the input.
        if parser.peek() == Some('\u{feff}') {
            parser.advance();
        }
        parser
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
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
            .all(|(k, ch)| self.chars.get(self.i + k) == Some(&ch))
    }

    fn error(&self, message: impl Into<String>) -> JsonError {
        JsonError::at(self.source, message, self.byte)
    }

    fn error_at(&self, message: impl Into<String>, offset: usize) -> JsonError {
        JsonError::at(self.source, message, offset)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.advance();
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error(format!("JSON nested more than {MAX_DEPTH} levels deep")));
        }
        self.skip_whitespace();
        match self.peek() {
            Some('{') => self.object(depth),
            Some('[') => self.array(depth),
            Some('"') => Ok(Json::String(self.string()?)),
            Some('t') | Some('f') => self.boolean(),
            Some('n') => {
                self.literal("null")?;
                Ok(Json::Null)
            }
            Some(ch) if ch == '-' || ch.is_ascii_digit() => self.number().map(Json::Number),
            Some(ch) => Err(self.error(format!("unexpected character `{ch}`"))),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn literal(&mut self, word: &str) -> Result<(), JsonError> {
        if self.looking_at(word) {
            for _ in word.chars() {
                self.advance();
            }
            Ok(())
        } else {
            Err(self.error(format!("expected `{word}`")))
        }
    }

    fn boolean(&mut self) -> Result<Json, JsonError> {
        if self.peek() == Some('t') {
            self.literal("true")?;
            Ok(Json::Bool(true))
        } else {
            self.literal("false")?;
            Ok(Json::Bool(false))
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.advance(); // `{`
        let mut entries: Vec<(String, Json)> = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some('}') {
            self.advance();
            return Ok(Json::Object(entries));
        }
        loop {
            self.skip_whitespace();
            if self.peek().is_none() {
                return Err(self.error("unterminated object"));
            }
            if self.peek() != Some('"') {
                return Err(self.error("expected a string key"));
            }
            let key_offset = self.byte;
            let key = self.string()?;
            if entries.iter().any(|(known, _)| *known == key) {
                return Err(self.error_at(format!("duplicate object key `{key}`"), key_offset));
            }
            self.skip_whitespace();
            if self.advance() != Some(':') {
                return Err(self.error("expected `:` after the object key"));
            }
            let value = self.value(depth + 1)?;
            entries.push((key, value));
            self.skip_whitespace();
            match self.advance() {
                Some(',') => continue,
                Some('}') => return Ok(Json::Object(entries)),
                Some(ch) => {
                    return Err(self.error(format!("expected `,` or `}}`, found `{ch}`")));
                }
                None => return Err(self.error("unterminated object")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.advance(); // `[`
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(']') {
            self.advance();
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.advance() {
                Some(',') => continue,
                Some(']') => return Ok(Json::Array(items)),
                Some(ch) => {
                    return Err(self.error(format!("expected `,` or `]`, found `{ch}`")));
                }
                None => return Err(self.error("unterminated array")),
            }
        }
    }

    fn string(&mut self) -> Result<String, JsonError> {
        debug_assert_eq!(self.peek(), Some('"'));
        self.advance();
        let mut out = String::new();
        loop {
            let Some(ch) = self.advance() else {
                return Err(self.error("unterminated string"));
            };
            match ch {
                '"' => return Ok(out),
                '\\' => out.push(self.escape()?),
                // Unescaped control characters are not valid JSON.
                ch if (ch as u32) < 0x20 => {
                    return Err(self.error_at(
                        format!("unescaped control character U+{:04X} in string", ch as u32),
                        self.byte - ch.len_utf8(),
                    ));
                }
                ch => out.push(ch),
            }
        }
    }

    /// Decode one escape sequence; the backslash is already consumed.
    fn escape(&mut self) -> Result<char, JsonError> {
        let offset = self.byte - 1;
        let Some(ch) = self.advance() else {
            return Err(self.error_at("unterminated escape sequence", offset));
        };
        Ok(match ch {
            '"' => '"',
            '\\' => '\\',
            '/' => '/',
            'b' => '\u{08}',
            'f' => '\u{0c}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'u' => self.unicode_escape(offset)?,
            other => {
                return Err(self.error_at(format!("invalid escape `\\{other}`"), offset));
            }
        })
    }

    /// Decode a `\uXXXX` escape, joining a surrogate pair when present.
    fn unicode_escape(&mut self, offset: usize) -> Result<char, JsonError> {
        let high = self.hex4(offset)?;
        // A high surrogate must be followed by a low surrogate.
        if (0xD800..0xDC00).contains(&high) {
            if self.advance() != Some('\\') || self.advance() != Some('u') {
                return Err(self.error_at(
                    format!("high surrogate U+{high:04X} is not followed by a low surrogate"),
                    offset,
                ));
            }
            let low = self.hex4(offset)?;
            if !(0xDC00..0xE000).contains(&low) {
                return Err(self.error_at(format!("U+{low:04X} is not a low surrogate"), offset));
            }
            let combined = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
            return char::from_u32(combined).ok_or_else(|| {
                self.error_at(
                    format!("U+{combined:04X} is not a valid code point"),
                    offset,
                )
            });
        }
        if (0xDC00..0xE000).contains(&high) {
            return Err(self.error_at(format!("unexpected low surrogate U+{high:04X}"), offset));
        }
        char::from_u32(high)
            .ok_or_else(|| self.error_at(format!("U+{high:04X} is not a valid code point"), offset))
    }

    fn hex4(&mut self, offset: usize) -> Result<u32, JsonError> {
        let mut value = 0u32;
        for _ in 0..4 {
            let Some(ch) = self.advance() else {
                return Err(self.error_at("truncated `\\u` escape", offset));
            };
            let Some(digit) = ch.to_digit(16) else {
                return Err(self.error_at(
                    format!("`{ch}` is not a hexadecimal digit in a `\\u` escape"),
                    offset,
                ));
            };
            value = value * 16 + digit;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<f64, JsonError> {
        let start = self.byte;
        let start_i = self.i;

        if self.peek() == Some('-') {
            self.advance();
        }

        match self.peek() {
            Some('0') => {
                self.advance();
                if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    return Err(self.error_at("leading zeros are not allowed", start));
                }
            }
            Some(ch) if ch.is_ascii_digit() => {
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.advance();
                }
            }
            _ => return Err(self.error("expected a digit")),
        }

        if self.peek() == Some('.') {
            self.advance();
            if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                return Err(self.error("expected a digit after `.`"));
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }

        if matches!(self.peek(), Some('e') | Some('E')) {
            self.advance();
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.advance();
            }
            if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                return Err(self.error("expected a digit in the exponent"));
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }

        let text: String = self.chars[start_i..self.i].iter().collect();
        let value: f64 = text
            .parse()
            .map_err(|_| self.error_at(format!("invalid number `{text}`"), start))?;
        if !value.is_finite() {
            return Err(self.error_at(format!("number `{text}` is out of range"), start));
        }
        Ok(value)
    }
}

// ---------------------------------------------------------------------------
// Writing

/// Serialize a value compactly.
pub fn to_string(value: &Json) -> String {
    let mut out = String::new();
    write_compact(&mut out, value);
    out
}

/// Serialize a value with two-space indentation.
pub fn to_string_pretty(value: &Json) -> String {
    let mut out = String::new();
    write_pretty(&mut out, value, 0);
    out
}

/// Escape `text` as a JSON string literal, including the surrounding quotes.
pub fn quote(text: &str) -> String {
    let mut out = String::new();
    write_string(&mut out, text);
    out
}

fn write_compact(out: &mut String, value: &Json) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Number(number) => out.push_str(&format_number(*number)),
        Json::String(text) => write_string(out, text),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_compact(out, item);
            }
            out.push(']');
        }
        Json::Object(entries) => {
            out.push('{');
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(out, key);
                out.push(':');
                write_compact(out, item);
            }
            out.push('}');
        }
    }
}

fn write_pretty(out: &mut String, value: &Json, depth: usize) {
    match value {
        Json::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                indent(out, depth + 1);
                write_pretty(out, item, depth + 1);
                if index + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push(']');
        }
        Json::Object(entries) if !entries.is_empty() => {
            out.push_str("{\n");
            for (index, (key, item)) in entries.iter().enumerate() {
                indent(out, depth + 1);
                write_string(out, key);
                out.push_str(": ");
                write_pretty(out, item, depth + 1);
                if index + 1 < entries.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push('}');
        }
        other => write_compact(out, other),
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
}

/// Format a finite `f64` as JSON. Rust's `Display` never uses exponent
/// notation, so the result is always a valid JSON number.
fn format_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    // JSON has no way to spell an infinity or a NaN, and `format!` would write
    // `inf`/`NaN`, which no JSON reader accepts. Nothing that *parses* JSON can
    // reach this (a document cannot contain them), so a non-finite value here
    // can only have been computed — and the machine raises `Math ERROR` rather
    // than producing one. Emitting `null` keeps the output parseable instead of
    // handing a consumer something that is not JSON at all.
    if !value.is_finite() {
        return "null".to_string();
    }
    format!("{value}")
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn number(text: &str) -> f64 {
        parse(text).unwrap().as_f64().unwrap()
    }

    #[test]
    fn parses_scalars() {
        assert_eq!(parse("null").unwrap(), Json::Null);
        assert_eq!(parse(" true ").unwrap(), Json::Bool(true));
        assert_eq!(parse("false").unwrap(), Json::Bool(false));
        assert_eq!(number("42"), 42.0);
        assert_eq!(number("-0.5"), -0.5);
        assert_eq!(number("1e3"), 1000.0);
        assert_eq!(number("2.5E-2"), 0.025);
        assert_eq!(parse(r#""hi""#).unwrap(), Json::String("hi".into()));
    }

    #[test]
    fn booleans_widen_to_numbers() {
        assert_eq!(parse("true").unwrap().as_number(), Some(1.0));
        assert_eq!(parse("false").unwrap().as_number(), Some(0.0));
        assert_eq!(parse(r#""x""#).unwrap().as_number(), None);
    }

    #[test]
    fn parses_nested_containers_in_order() {
        let value = parse(r#"{"b": 1, "a": [1, {"c": 2}]}"#).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(keys, ["b", "a"], "document order is preserved");
        assert_eq!(
            value
                .get("a")
                .and_then(|a| a.index(1))
                .and_then(|o| o.get("c"))
                .and_then(Json::as_f64),
            Some(2.0)
        );
    }

    #[test]
    fn parses_empty_containers() {
        assert_eq!(parse("[]").unwrap(), Json::Array(vec![]));
        assert_eq!(parse("{}").unwrap(), Json::Object(vec![]));
        assert_eq!(parse("[ ]").unwrap(), Json::Array(vec![]));
    }

    #[test]
    fn decodes_string_escapes() {
        let value = parse(r#""a\n\t\"\\\/\u0041""#).unwrap();
        assert_eq!(value.as_str(), Some("a\n\t\"\\/A"));
    }

    #[test]
    fn decodes_surrogate_pairs() {
        // U+1F600 GRINNING FACE.
        assert_eq!(
            parse(r#""\uD83D\uDE00""#).unwrap().as_str(),
            Some("\u{1f600}")
        );
    }

    #[test]
    fn rejects_lone_surrogates() {
        assert!(parse(r#""\uD83D""#).is_err());
        assert!(parse(r#""\uDE00""#).is_err());
        assert!(parse(r#""\uD83Dx""#).is_err());
    }

    #[test]
    fn rejects_duplicate_keys() {
        let err = parse(r#"{"a": 1, "a": 2}"#).unwrap_err();
        assert!(err.message.contains("duplicate object key"), "{err}");
    }

    #[test]
    fn duplicate_checks_are_per_object_not_global() {
        let value = parse(r#"[{"a": 1}, {"a": 2}]"#).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 2);
    }

    #[test]
    fn rejects_malformed_documents() {
        for (text, needle) in [
            ("{", "unterminated"),
            ("[1, 2", "unterminated"),
            ("[1,]", "unexpected character `]`"),
            (r#"{"a": 1,}"#, "expected a string key"),
            (r#"{a: 1}"#, "expected a string key"),
            (r#"{"a" 1}"#, "expected `:`"),
            ("01", "leading zeros"),
            ("1.", "expected a digit after `.`"),
            (".5", "unexpected character `.`"),
            ("+1", "unexpected character `+`"),
            ("1e", "expected a digit in the exponent"),
            ("nul", "expected `null`"),
            ("tru", "expected `true`"),
            (r#""abc"#, "unterminated string"),
            (r#""a\qb""#, "invalid escape"),
            (r#""\u12g4""#, "not a hexadecimal digit"),
            ("1 2", "trailing character"),
            ("", "unexpected end of input"),
            ("1e400", "out of range"),
        ] {
            let err = parse(text).unwrap_err();
            assert!(
                err.message.contains(needle),
                "parsing {text:?}: expected {needle:?}, got {:?}",
                err.message
            );
        }
    }

    #[test]
    fn rejects_raw_control_characters_in_strings() {
        let err = parse("\"a\nb\"").unwrap_err();
        assert!(err.message.contains("control character"), "{err}");
    }

    #[test]
    fn errors_carry_a_line_and_column() {
        let err = parse("{\n  \"a\": 01\n}").unwrap_err();
        assert_eq!(err.line, 2);
        assert_eq!(err.column, 8);
    }

    #[test]
    fn nesting_is_capped() {
        let deep = format!(
            "{}1{}",
            "[".repeat(MAX_DEPTH + 2),
            "]".repeat(MAX_DEPTH + 2)
        );
        let err = parse(&deep).unwrap_err();
        assert!(err.message.contains("nested more than"), "{err}");
    }

    #[test]
    fn a_bom_is_skipped_without_shifting_offsets() {
        let value = parse("\u{feff}[1]").unwrap();
        assert_eq!(value.index(0).and_then(Json::as_f64), Some(1.0));
    }

    #[test]
    fn pretty_output_round_trips() {
        let source = r#"{"a":[1,{"b":null},"x\ny"],"c":true}"#;
        let value = parse(source).unwrap();
        let pretty = to_string_pretty(&value);
        assert_eq!(parse(&pretty).unwrap(), value);
        assert_eq!(parse(&to_string(&value)).unwrap(), value);
        assert!(pretty.contains("\n  \"a\": ["), "{pretty}");
    }

    #[test]
    fn writing_escapes_control_characters_and_quotes() {
        let value = Json::String("a\"b\\c\nd\u{1}".into());
        let text = to_string(&value);
        assert_eq!(text, r#""a\"b\\c\nd\u0001""#);
        assert_eq!(parse(&text).unwrap(), value);
    }

    #[test]
    fn numbers_round_trip_through_text() {
        for value in [0.0, -0.0, 1.0, -1.5, 1e300, 1e-300, 123456.789] {
            let text = to_string(&Json::Number(value));
            let back = parse(&text).unwrap().as_f64().unwrap();
            assert_eq!(back, value, "{text}");
        }
        assert_eq!(to_string(&Json::Number(0.0)), "0");
    }

    #[test]
    fn prefix_parsing_stops_after_one_value() {
        let (value, used) = parse_prefix("  [1, 2] ; rest").unwrap();
        assert_eq!(value.index(1).and_then(Json::as_f64), Some(2.0));
        assert_eq!(&"  [1, 2] ; rest"[used..], " ; rest");
        let (value, used) = parse_prefix("-12.5;").unwrap();
        assert_eq!(value.as_f64(), Some(-12.5));
        assert_eq!(used, 5);
        assert!(parse_prefix("").is_err());
    }

    #[test]
    fn builders_and_type_names() {
        let value = Json::object([
            ("n", Json::Number(1.0)),
            ("xs", Json::array([Json::Bool(true)])),
        ]);
        assert_eq!(value.get("xs").unwrap().type_name(), "array");
        assert_eq!(value.get("n").unwrap().type_name(), "number");
        assert_eq!(Json::Null.type_name(), "null");
        assert_eq!(Json::string("x").as_str(), Some("x"));
    }

    #[test]
    fn a_non_finite_number_is_written_as_null_not_as_inf() {
        // `inf`/`NaN` are not JSON, and a reader would reject the whole
        // document. `null` is the only representable answer.
        assert_eq!(to_string(&Json::Number(f64::INFINITY)), "null");
        assert_eq!(to_string(&Json::Number(f64::NEG_INFINITY)), "null");
        assert_eq!(to_string(&Json::Number(f64::NAN)), "null");
        // Finite numbers are untouched, including the zero special case.
        assert_eq!(to_string(&Json::Number(0.0)), "0");
        assert_eq!(to_string(&Json::Number(-0.0)), "0");
        assert_eq!(to_string(&Json::Number(1.5)), "1.5");
    }
}
