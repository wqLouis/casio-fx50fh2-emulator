//! Strict JSON parsing on top of `serde_json`.
//!
//! `#data` compiles a JSON document into a calculator program, so a silent
//! reinterpretation is worse than a clear error. `serde_json` already gives us
//! RFC 8259 syntax (no comments, no trailing commas), the 128-level recursion
//! cap, finite-only numbers and lone-surrogate rejection. On top of that this
//! module keeps the two guarantees the language actually relies on:
//!
//! * **duplicate object keys are an error.** `serde_json` keeps the last one,
//!   which would let a `#data` typo change a value without a word. The custom
//!   visitor below checks each object as it is built.
//! * **every number is an `f64`.** A parsed integer is widened immediately, so
//!   the value a program reads is exactly what the calculator's 15-digit
//!   arithmetic will compute with. (This is also why the number stored for
//!   `9007199254740993` is `9007199254740992`: an `f64` cannot hold the former,
//!   which is the same rounding the machine applies.)
//!
//! Two things deliberately differ from the hand-written parser this replaced:
//!
//! * **object order is not preserved.** `serde_json`'s map is a sorted
//!   `BTreeMap`, so a data table's fields come back in key order rather than in
//!   document order. Nothing in the crate depends on the order — `Data::resolve`
//!   looks fields up by name — and keeping document order would mean enabling
//!   `serde_json/preserve_order` and pulling in `indexmap` for no reader.
//! * **error text is `serde_json`'s**, with its position stripped and replaced
//!   by our own byte offset, line and column, so a nested JSON file still maps
//!   back to the exact spot in the `#data` directive.
//!
//! The parser is deliberately not public: the crate now uses `serde_json`'s
//! `Value` directly, and only `#data`/`#tests` need the extra strictness.

use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

/// A JSON parse failure, positioned against the text handed to [`parse`] or
/// [`parse_prefix`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JsonError {
    /// The failure, without a trailing position (that is in the fields).
    pub(crate) message: String,
    /// Byte offset into the input.
    pub(crate) offset: usize,
    /// 1-based line number.
    pub(crate) line: usize,
    /// 1-based column, counted in `char`s.
    pub(crate) column: usize,
}

impl JsonError {
    fn at(source: &str, message: impl Into<String>, offset: usize) -> Self {
        let (line, column) = crate::error::line_col(source, offset);
        JsonError {
            message: message.into(),
            offset,
            line,
            column,
        }
    }
}

/// A short human-readable name for a value's type, for diagnostics.
pub(crate) fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The value as a number: booleans widen to `1`/`0`, matching the calculator,
/// which has no boolean type.
pub(crate) fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::Bool(true) => Some(1.0),
        Value::Bool(false) => Some(0.0),
        _ => None,
    }
}

/// Parse a complete JSON document.
///
/// A leading UTF-8 BOM is skipped; trailing non-whitespace is an error, because
/// a `#data` value is expected to end where it says it does.
pub(crate) fn parse(text: &str) -> Result<Value, JsonError> {
    let (bom, body) = split_bom(text);
    serde_json::from_str::<StrictValue>(body)
        .map(|value| value.0)
        .map_err(|error| convert(error, body, bom, text))
}

/// Parse one JSON value from the start of `text`, returning it together with
/// the byte offset just past it.
///
/// The value ends where the JSON grammar says it does and whatever follows is
/// left to the caller — this is what a `#data NAME = VALUE;` directive needs,
/// because the `;` is not part of the value. Trailing text is never an error
/// here, so `parse_prefix("1;")` yields `(1, 1)`.
pub(crate) fn parse_prefix(text: &str) -> Result<(Value, usize), JsonError> {
    let (bom, body) = split_bom(text);
    let lead = body.len() - body.trim_start_matches(WHITESPACE).len();
    let rest = &body[lead..];

    match rest.as_bytes().first() {
        None => Err(JsonError::at(text, "unexpected end of input", bom + lead)),
        // A self-delineating value (`[`, `{`, `"`) ends itself, and
        // `serde_json`'s stream iterator knows exactly where. It deliberately
        // does not require a value terminator after one, which is what lets a
        // `#data` array or object be followed by `;`.
        Some(b'[') | Some(b'{') | Some(b'"') => {
            let mut stream = serde_json::Deserializer::from_str(body).into_iter::<StrictValue>();
            let value = stream
                .next()
                .transpose()
                .map_err(|error| convert(error, body, bom, text))?
                .ok_or_else(|| JsonError::at(text, "unexpected end of input", bom + lead))?;
            Ok((value.0, bom + stream.byte_offset()))
        }
        // A scalar (number, `true`, `false`, `null`) has no closing token, so
        // its end is found lexically and the slice is parsed as a document.
        Some(_) => {
            let length = scalar_len(rest);
            let slice = &rest[..length];
            let value = serde_json::from_str::<StrictValue>(slice)
                .map(|value| value.0)
                .map_err(|error| convert(error, slice, bom + lead, text))?;
            Ok((value, bom + lead + length))
        }
    }
}

/// The JSON whitespace set (RFC 8259 §2).
const WHITESPACE: [char; 4] = [' ', '\t', '\n', '\r'];

/// Split a leading byte-order mark off `text`, returning its byte length.
fn split_bom(text: &str) -> (usize, &str) {
    match text.strip_prefix('\u{feff}') {
        Some(rest) => (3, rest),
        None => (0, text),
    }
}

/// The byte length of the scalar starting `text`, which must start with one.
///
/// This is only a boundary finder; the slice it produces is handed to
/// `serde_json`, which is what actually validates it and reports a good error
/// for `+1`, `.5`, `nul`, `1e`, `trueX` and the like.
fn scalar_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    let take = |set: fn(&u8) -> bool| bytes.iter().take_while(|byte| set(byte)).count();
    match bytes.first() {
        Some(b'-' | b'0'..=b'9') => {
            take(|byte| matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        }
        Some(b't' | b'f' | b'n') => take(u8::is_ascii_alphabetic),
        // Anything else is malformed; a one-byte slice lets `serde_json`
        // report it without this function having to know every failure mode.
        _ => 1,
    }
}

/// Map a `serde_json` error against `text` into this module's error type.
///
/// `text` is the string that was parsed, `base` its byte offset inside the
/// `original` the caller handed in (non-zero only after a BOM), and `original`
/// is what line/column are reported against.
fn convert(error: serde_json::Error, text: &str, base: usize, original: &str) -> JsonError {
    let offset = base + offset_of_line_col(text, error.line(), error.column());
    JsonError::at(original, strip_position(&error), offset)
}

/// Map a 1-based line/column (with a byte-counted column, as `serde_json`
/// reports) back to a byte offset. Offsets past the end of the input clamp to
/// the end.
fn offset_of_line_col(text: &str, line: usize, column: usize) -> usize {
    let bytes = text.as_bytes();
    let mut current = 1usize;
    let mut line_start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() && current < line {
        if bytes[index] == b'\n' {
            current += 1;
            line_start = index + 1;
        }
        index += 1;
    }
    (line_start + column.saturating_sub(1)).min(text.len())
}

/// Strip `serde_json`'s trailing ` at line L column C` from a display message,
/// because the position lives in dedicated fields and is rendered by
/// [`JsonError`] itself.
fn strip_position(error: &serde_json::Error) -> String {
    let full = error.to_string();
    let suffix = format!(" at line {} column {}", error.line(), error.column());
    match full.strip_suffix(&suffix) {
        Some(message) => message.to_string(),
        None => full,
    }
}

// ---------------------------------------------------------------------------
// The strict value visitor

/// A [`Value`] built through a visitor that rejects duplicate object keys and
/// widens every number to `f64`.
struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor)
    }
}

struct StrictValueVisitor;

impl<'de> Visitor<'de> for StrictValueVisitor {
    type Value = StrictValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any valid JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        float(value as f64)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        float(value as f64)
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        float(value)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value)))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::new();
        while let Some(item) = sequence.next_element::<StrictValue>()? {
            items.push(item.0);
        }
        Ok(StrictValue(Value::Array(items)))
    }

    fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut entries = Map::new();
        while let Some(key) = access.next_key::<String>()? {
            if entries.contains_key(&key) {
                return Err(de::Error::custom(format!("duplicate object key `{key}`")));
            }
            let value = access.next_value::<StrictValue>()?;
            entries.insert(key, value.0);
        }
        Ok(StrictValue(Value::Object(entries)))
    }
}

/// Build a number value, rejecting a non-finite one.
///
/// `serde_json`'s parser already refuses to produce an infinity (an
/// out-of-range literal such as `1e400` is an error, not `inf`), so this is a
/// belt-and-braces conversion that keeps the "parse never yields `inf`/`NaN`"
/// guarantee explicit.
fn float<E: de::Error>(value: f64) -> Result<StrictValue, E> {
    serde_json::Number::from_f64(value)
        .map(|number| StrictValue(Value::Number(number)))
        .ok_or_else(|| E::custom(format!("number `{value}` is out of range")))
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
        assert_eq!(parse("null").unwrap(), Value::Null);
        assert_eq!(parse(" true ").unwrap(), Value::Bool(true));
        assert_eq!(parse("false").unwrap(), Value::Bool(false));
        assert_eq!(number("42"), 42.0);
        assert_eq!(number("-0.5"), -0.5);
        assert_eq!(number("1e3"), 1000.0);
        assert_eq!(number("2.5E-2"), 0.025);
        assert_eq!(parse(r#""hi""#).unwrap(), Value::String("hi".into()));
    }

    #[test]
    fn numbers_are_always_f64() {
        // The stored variant is a float, not an integer, which is what makes
        // the value a program reads match the calculator's arithmetic.
        assert_eq!(parse("3").unwrap(), Value::from(3.0));
        // Past 2^53 the `f64` rounds, as intended.
        assert_eq!(number("9007199254740993"), 9007199254740992.0);
    }

    #[test]
    fn booleans_widen_to_numbers() {
        assert_eq!(as_number(&parse("true").unwrap()), Some(1.0));
        assert_eq!(as_number(&parse("false").unwrap()), Some(0.0));
        assert_eq!(as_number(&parse(r#""x""#).unwrap()), None);
    }

    #[test]
    fn parses_nested_containers() {
        let value = parse(r#"{"b": 1, "a": [1, {"c": 2}]}"#).unwrap();
        assert_eq!(
            value
                .get("a")
                .and_then(|a| a.get(1))
                .and_then(|o| o.get("c"))
                .and_then(Value::as_f64),
            Some(2.0)
        );
    }

    #[test]
    fn parses_empty_containers() {
        assert_eq!(parse("[]").unwrap(), Value::Array(vec![]));
        assert_eq!(parse("{}").unwrap(), Value::Object(Map::new()));
        assert_eq!(parse("[ ]").unwrap(), Value::Array(vec![]));
    }

    #[test]
    fn object_order_is_sorted_not_document_order() {
        // The deliberate relaxation from the hand-written parser: keys come
        // back in sorted order. Nothing in the crate depends on document order.
        let value = parse(r#"{"b": 1, "a": 2}"#).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["a", "b"]);
    }

    #[test]
    fn decodes_string_escapes_and_surrogate_pairs() {
        assert_eq!(
            parse(r#""a\n\t\"\\\/\u0041""#).unwrap().as_str(),
            Some("a\n\t\"\\/A")
        );
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
        let error = parse(r#"{"a": 1, "a": 2}"#).unwrap_err();
        assert!(
            error.message.contains("duplicate object key `a`"),
            "{error:?}"
        );
        assert_eq!(error.line, 1);
    }

    #[test]
    fn duplicate_checks_are_per_object_not_global() {
        let value = parse(r#"[{"a": 1}, {"a": 2}]"#).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 2);
    }

    #[test]
    fn rejects_malformed_documents() {
        for text in [
            "{",
            "[1, 2",
            "[1,]",
            r#"{"a": 1,}"#,
            r#"{a: 1}"#,
            r#"{"a" 1}"#,
            "01",
            "1.",
            ".5",
            "+1",
            "1e",
            "nul",
            "tru",
            "\"abc",
            r#""a\qb""#,
            r#""\u12g4""#,
            "1 2",
            "",
        ] {
            assert!(parse(text).is_err(), "expected {text:?} to be rejected");
        }
    }

    #[test]
    fn rejects_out_of_range_numbers_rather_than_infinity() {
        for text in ["1e400", "-1e400", "1e309"] {
            let error = parse(text).unwrap_err();
            assert!(error.message.contains("out of range"), "{error:?}");
        }
    }

    #[test]
    fn rejects_raw_control_characters_in_strings() {
        assert!(parse("\"a\nb\"").is_err());
    }

    #[test]
    fn errors_carry_a_line_and_column() {
        let error = parse("{\n  \"a\": 01\n}").unwrap_err();
        assert!(error.line >= 2, "{error:?}");
        assert!(error.column >= 1, "{error:?}");
    }

    #[test]
    fn nesting_is_capped() {
        // `serde_json`'s default recursion limit is 128, the cap the module
        // documents; 200 levels must be a clear error, not a stack overflow.
        let deep = format!("{}1{}", "[".repeat(200), "]".repeat(200));
        let error = parse(&deep).unwrap_err();
        assert!(error.message.contains("recursion limit"), "{error:?}");
    }

    #[test]
    fn a_bom_is_skipped() {
        assert_eq!(
            parse("\u{feff}[1]").unwrap().get(0),
            Some(&Value::from(1.0))
        );
    }

    #[test]
    fn writing_a_non_finite_number_is_null_not_inf() {
        // `inf`/`NaN` are not JSON, and a reader would reject the whole
        // document. `serde_json` never constructs a non-finite `Number`, and
        // widening a non-finite `f64` yields `null` — the behaviour the old
        // hand-written writer pinned, now inherited. This test keeps the
        // guarantee from silently disappearing.
        assert_eq!(
            serde_json::to_string(&Value::from(f64::INFINITY)).unwrap(),
            "null"
        );
        assert_eq!(
            serde_json::to_string(&Value::from(f64::NEG_INFINITY)).unwrap(),
            "null"
        );
        assert_eq!(
            serde_json::to_string(&Value::from(f64::NAN)).unwrap(),
            "null"
        );
    }

    #[test]
    fn prefix_parsing_stops_after_one_value() {
        let (value, used) = parse_prefix("  [1, 2] ; rest").unwrap();
        assert_eq!(value.get(1), Some(&Value::from(2.0)));
        assert_eq!(&"  [1, 2] ; rest"[used..], " ; rest");

        let (value, used) = parse_prefix("-12.5;").unwrap();
        assert_eq!(value.as_f64(), Some(-12.5));
        assert_eq!(used, 5);

        let (value, used) = parse_prefix("1;").unwrap();
        assert_eq!(value.as_f64(), Some(1.0));
        assert_eq!(used, 1);

        let (value, used) = parse_prefix(r#"{"a": true}; trailing"#).unwrap();
        assert_eq!(value.get("a"), Some(&Value::Bool(true)));
        assert_eq!(&r#"{"a": true}; trailing"#[used..], "; trailing");

        assert!(parse_prefix("").is_err());
        assert!(parse_prefix("   ").is_err());
    }

    #[test]
    fn prefix_errors_are_positioned_in_the_original_text() {
        // Leading whitespace ahead of a malformed scalar must not shift the
        // reported offset.
        let error = parse_prefix("  +1").unwrap_err();
        assert_eq!(error.offset, 2);
        assert_eq!(error.column, 3);
        assert!(error.message.contains("expected value"), "{error:?}");
    }
}
