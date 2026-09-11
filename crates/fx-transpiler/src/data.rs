//! Compile-time data: the `#data` and `#tests` directives.
//!
//! This is the general facility, not a test feature. A program can read a JSON
//! file — or inline a JSON literal — while it is being transpiled, and use the
//! values as constants in the generated PRGM:
//!
//! ```text
//! #data config  = { "size": 3, "weights": [1, 2, 3] };
//! #data offsets = "offsets.json";
//!
//! const size = config.size;
//! let total = config.weights[0] * offsets.scale;
//! ```
//!
//! Nothing of the JSON reaches the calculator: `config.size` and
//! `offsets.scale` are resolved here and emitted as number literals, and only
//! numbers and booleans can be used in a program at all. The JSON itself is
//! parsed by [`crate::json`], the crate's single JSON implementation.
//!
//! `#tests` is exactly `#data tests = ...`, so the test runner is an ordinary
//! consumer of this facility rather than a special case.
//!
//! ## Rules
//!
//! * A directive is `#data NAME = VALUE;` or `#tests = VALUE;`, and must be
//!   the first thing on its line. `VALUE` is JSON and may span lines.
//! * A **top-level JSON string** means "read this file and use its contents",
//!   which is why `#tests = "cases.json";` works. Strings nested inside
//!   objects or arrays stay strings.
//! * A file path resolves relative to the file containing the directive, so a
//!   library fragment can ship its own data.
//! * Names are program-global and must be unique.
//! * Extraction happens after `#include` expansion but before lexing. The
//!   directive text is blanked out — newlines preserved — so every line number
//!   still refers to the line the user wrote.

use std::path::{Path, PathBuf};

use crate::ast::Accessor;
use crate::error::TranspileError;
use crate::include::Expanded;
use crate::json::{self, Json};

/// Names a data table may not use, because the language already claims them.
const RESERVED: [&str; 14] = [
    "let", "const", "if", "else", "while", "for", "break", "goto", "label", "print", "phys", "pi",
    "e", "input",
];

/// One declared data table.
#[derive(Debug, Clone, PartialEq)]
pub struct DataTable {
    /// The name the program refers to it by.
    pub name: String,
    /// Byte offset of the `#data` directive in the expanded source.
    pub offset: usize,
    /// The JSON value, already resolved (a file reference has been read).
    pub value: Json,
}

/// Every compile-time data table in a program, in declaration order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Data {
    tables: Vec<DataTable>,
}

impl Data {
    /// Look a table up by name.
    pub fn get(&self, name: &str) -> Option<&DataTable> {
        self.tables.iter().find(|table| table.name == name)
    }

    /// Whether `name` is a declared table.
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// The declared tables, in order.
    pub fn iter(&self) -> impl Iterator<Item = &DataTable> {
        self.tables.iter()
    }

    /// The declared names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.tables.iter().map(|table| table.name.as_str())
    }

    pub fn len(&self) -> usize {
        self.tables.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// The `#tests` table, if the program declares one.
    pub fn tests(&self) -> Option<&Json> {
        self.get("tests").map(|table| &table.value)
    }

    /// Resolve a data path such as `config.size` or `weights[0]` to the number
    /// the program will use.
    ///
    /// `pos` points at the name, so a bad path is reported where it was
    /// written; each accessor carries its own offset for a precise column.
    pub fn resolve(
        &self,
        name: &str,
        accessors: &[Accessor],
        source: &str,
        pos: usize,
    ) -> Result<f64, TranspileError> {
        let table = self.get(name).ok_or_else(|| {
            TranspileError::at(source, format!("unknown data table `{name}`"), pos)
        })?;
        let mut value = &table.value;
        let mut path = name.to_string();

        for accessor in accessors {
            match accessor {
                Accessor::Field {
                    name: field,
                    pos: at,
                } => {
                    let Json::Object(entries) = value else {
                        return Err(TranspileError::at(
                            source,
                            format!("`{path}` is {} and has no field `{field}`", describe(value)),
                            *at,
                        ));
                    };
                    let Some((_, next)) = entries.iter().find(|(key, _)| key == field) else {
                        let keys: Vec<&str> = entries.iter().map(|(key, _)| key.as_str()).collect();
                        let hint = if keys.is_empty() {
                            "it has no fields".to_string()
                        } else {
                            format!("fields: {}", keys.join(", "))
                        };
                        return Err(TranspileError::at(
                            source,
                            format!("`{path}` has no field `{field}` ({hint})"),
                            *at,
                        ));
                    };
                    value = next;
                    path.push('.');
                    path.push_str(field);
                }
                Accessor::IndexExpr { pos: at, .. } => {
                    return Err(crate::error::computed_index_error(source, name, *at));
                }
                Accessor::Index { index, pos: at } => {
                    let Json::Array(items) = value else {
                        return Err(TranspileError::at(
                            source,
                            format!("`{path}` is {} and cannot be indexed", describe(value)),
                            *at,
                        ));
                    };
                    let Some(next) = items.get(*index) else {
                        return Err(TranspileError::at(
                            source,
                            format!(
                                "index {index} is out of range for `{path}` (length {})",
                                items.len()
                            ),
                            *at,
                        ));
                    };
                    value = next;
                    path.push_str(&format!("[{index}]"));
                }
            }
        }

        value.as_number().ok_or_else(|| {
            TranspileError::at(
                source,
                format!(
                    "`{path}` is {}; only numbers and booleans can be used in a program",
                    describe(value)
                ),
                pos,
            )
        })
    }
}

/// `a number`, `an object`, … for diagnostics.
fn describe(value: &Json) -> String {
    let kind = value.type_name();
    match kind {
        "array" | "object" => format!("an {kind}"),
        "null" => "null".to_string(),
        _ => format!("a {kind}"),
    }
}

/// Remove `#data`/`#tests` directives from `expanded`, returning the text to
/// lex and the tables they declared.
///
/// `base_dir` resolves file references in an anonymous root; a directive that
/// came from an included file resolves against that file instead.
pub fn extract(expanded: &Expanded, base_dir: &Path) -> Result<(String, Data), TranspileError> {
    let mut extractor = Extractor::new(expanded, base_dir);
    extractor.run()?;
    Ok((extractor.out, extractor.data))
}

struct Extractor<'a> {
    source: &'a str,
    chars: Vec<char>,
    /// Byte offset of each char, plus one past the end.
    offsets: Vec<usize>,
    expanded: &'a Expanded,
    base_dir: PathBuf,
    i: usize,
    out: String,
    data: Data,
}

impl<'a> Extractor<'a> {
    fn new(expanded: &'a Expanded, base_dir: &Path) -> Self {
        let source = expanded.text.as_str();
        let chars: Vec<char> = source.chars().collect();
        let mut offsets = Vec::with_capacity(chars.len() + 1);
        let mut byte = 0usize;
        for ch in &chars {
            offsets.push(byte);
            byte += ch.len_utf8();
        }
        offsets.push(byte);
        Extractor {
            source,
            chars,
            offsets,
            expanded,
            base_dir: base_dir.to_path_buf(),
            i: 0,
            out: String::with_capacity(source.len()),
            data: Data::default(),
        }
    }

    fn byte(&self, index: usize) -> usize {
        self.offsets[index.min(self.offsets.len() - 1)]
    }

    fn run(&mut self) -> Result<(), TranspileError> {
        // Whether anything other than whitespace, comments or directives has
        // been seen on the current line. A directive must be first.
        let mut line_has_content = false;
        let mut in_line_comment = false;
        let mut in_block_comment = false;

        while self.i < self.chars.len() {
            let ch = self.chars[self.i];

            if ch == '\n' {
                self.push(ch);
                line_has_content = false;
                in_line_comment = false;
                continue;
            }
            if in_line_comment {
                self.push(ch);
                continue;
            }
            if in_block_comment {
                if ch == '*' && self.chars.get(self.i + 1) == Some(&'/') {
                    self.push('*');
                    self.push('/');
                    in_block_comment = false;
                    continue;
                }
                self.push(ch);
                continue;
            }
            if ch.is_whitespace() {
                self.push(ch);
                continue;
            }
            if ch == '/' && self.chars.get(self.i + 1) == Some(&'/') {
                in_line_comment = true;
                line_has_content = true;
                self.push('/');
                self.push('/');
                continue;
            }
            if ch == '/' && self.chars.get(self.i + 1) == Some(&'*') {
                in_block_comment = true;
                line_has_content = true;
                self.push('/');
                self.push('*');
                continue;
            }
            if ch == '#' && !line_has_content && self.directive()? {
                line_has_content = true;
                continue;
            }
            line_has_content = true;
            self.push(ch);
        }
        Ok(())
    }

    /// Push one char, advancing.
    fn push(&mut self, ch: char) {
        self.out.push(ch);
        self.i += 1;
    }

    /// Try to consume a `#data`/`#tests` directive at the current position.
    ///
    /// Returns `false` (leaving the scanner untouched) when this is some other
    /// `#` directive, which the lexer handles.
    fn directive(&mut self) -> Result<bool, TranspileError> {
        let start = self.i;
        let start_byte = self.byte(start);

        let (name, after) = if self.word_at(start, "#data") {
            match self.declared_name(start + 5)? {
                Some((name, after)) => (name, after),
                None => return Ok(false),
            }
        } else if self.word_at(start, "#tests") {
            if self
                .chars
                .get(start + 6)
                .is_some_and(|c| !c.is_whitespace())
            {
                return Ok(false);
            }
            ("tests".to_string(), start + 6)
        } else {
            return Ok(false);
        };

        // `= VALUE`
        let mut cur = after;
        self.skip_inline(&mut cur);
        if self.chars.get(cur) != Some(&'=') {
            return Err(TranspileError::at(
                self.source,
                format!("expected `=` after `#data {name}`"),
                self.byte(cur),
            ));
        }
        cur += 1;
        while self.chars.get(cur).is_some_and(|c| c.is_whitespace()) {
            cur += 1;
        }

        // The JSON value, which may span lines.
        let value_byte = self.byte(cur);
        let (value, consumed) = json::parse_prefix(&self.source[value_byte..]).map_err(|e| {
            TranspileError::at(
                self.source,
                format!("invalid JSON in the `{name}` data table: {}", e.message),
                value_byte + e.offset,
            )
        })?;
        let after_value = value_byte + consumed;

        // `;`, then the rest of the line must be blank or a comment.
        let rest = &self.source[after_value..];
        let trimmed = rest.trim_start_matches([' ', '\t']);
        let Some(trailing) = trimmed.strip_prefix(';') else {
            return Err(TranspileError::at(
                self.source,
                format!("expected `;` to end the `{name}` data table"),
                after_value,
            ));
        };
        let trailing = trailing.trim_start_matches([' ', '\t']);
        if !(trailing.is_empty() || trailing.starts_with('\n') || trailing.starts_with("//")) {
            return Err(TranspileError::at(
                self.source,
                "unexpected text after the data directive",
                after_value,
            ));
        }

        // Resolve a top-level string as a file reference.
        let value = match value {
            Json::String(path) => self.load_file(&path, value_byte)?,
            other => other,
        };

        if RESERVED.contains(&name.as_str()) {
            return Err(TranspileError::at(
                self.source,
                format!("`{name}` is a reserved word and cannot name a data table"),
                start_byte,
            ));
        }
        if self.data.contains(&name) {
            return Err(TranspileError::at(
                self.source,
                format!("data table `{name}` is already declared"),
                start_byte,
            ));
        }
        self.data.tables.push(DataTable {
            name,
            offset: start_byte,
            value,
        });

        // Blank from `#` to the end of the line holding the `;`, keeping
        // newlines so every line number stays valid.
        let line_end = self.source[after_value..]
            .find('\n')
            .map(|k| after_value + k)
            .unwrap_or(self.source.len());
        while self.i < self.chars.len() && self.byte(self.i) < line_end {
            if self.chars[self.i] == '\n' {
                self.push('\n');
            } else {
                self.push(' ');
            }
        }
        Ok(true)
    }

    /// Read `#data NAME` and return the name plus the index just after it.
    ///
    /// Returns `None` when this is not a `#data` directive (the keyword is not
    /// followed by whitespace), so another `#` directive can be tried.
    fn declared_name(&self, start: usize) -> Result<Option<(String, usize)>, TranspileError> {
        if self.chars.get(start).is_some_and(|c| !c.is_whitespace()) {
            return Ok(None);
        }
        let mut cur = start;
        while self.chars.get(cur).is_some_and(|c| c.is_whitespace()) {
            cur += 1;
        }
        let name_start = cur;
        while self
            .chars
            .get(cur)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            cur += 1;
        }
        if cur == name_start {
            return Err(TranspileError::at(
                self.source,
                "expected a name after `#data`",
                self.byte(cur),
            ));
        }
        let name: String = self.chars[name_start..cur].iter().collect();
        if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return Err(TranspileError::at(
                self.source,
                format!("`{name}` is not a valid data table name"),
                self.byte(name_start),
            ));
        }
        Ok(Some((name, cur)))
    }

    /// Skip spaces and tabs (not newlines) from `cur`.
    fn skip_inline(&self, cur: &mut usize) {
        while matches!(self.chars.get(*cur), Some(' ') | Some('\t')) {
            *cur += 1;
        }
    }

    fn word_at(&self, start: usize, word: &str) -> bool {
        word.chars()
            .enumerate()
            .all(|(k, ch)| self.chars.get(start + k) == Some(&ch))
    }

    /// Read a JSON data file relative to the file that declared it.
    fn load_file(&self, path: &str, at: usize) -> Result<Json, TranspileError> {
        let line = crate::error::line_col(self.source, at).0;
        let dir = self
            .expanded
            .origin(line)
            .0
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.base_dir.clone());
        let target = dir.join(path);
        let text = std::fs::read_to_string(&target).map_err(|e| {
            TranspileError::at(
                self.source,
                format!("cannot read data file `{}`: {e}", target.display()),
                at,
            )
        })?;
        json::parse(&text)
            .map_err(|e| TranspileError::at(&text, e.message, e.offset).in_file(Some(&target)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::include;

    fn extract_text(source: &str) -> (String, Data) {
        let expanded = include::expand(source, None, Path::new(".")).unwrap();
        extract(&expanded, Path::new(".")).unwrap()
    }

    fn error(source: &str) -> TranspileError {
        let expanded = include::expand(source, None, Path::new(".")).unwrap();
        extract(&expanded, Path::new(".")).unwrap_err()
    }

    #[test]
    fn extracts_an_inline_table() {
        let (text, data) = extract_text("#data config = { \"n\": 3 };\nprint(1);\n");
        assert_eq!(data.len(), 1);
        assert_eq!(
            data.get("config").unwrap().value.get("n"),
            Some(&Json::Number(3.0))
        );
        // The text keeps its line count and shrinks only in width.
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("print(1);"));
        assert!(!text.contains("#data"));
    }

    #[test]
    fn blanking_preserves_every_line() {
        let source = "#data a = [\n  1,\n  2\n];\nlet x = 1;\n";
        let (text, data) = extract_text(source);
        assert_eq!(text.lines().count(), source.lines().count());
        assert_eq!(text.lines().last(), Some("let x = 1;"));
        assert_eq!(data.get("a").unwrap().value.as_array().unwrap().len(), 2);
    }

    #[test]
    fn tests_is_sugar_for_a_data_table_named_tests() {
        let (_, data) = extract_text("#tests = [{\"name\": \"a\"}];\n");
        assert_eq!(data.tests().unwrap().as_array().unwrap().len(), 1);
    }

    #[test]
    fn resolves_paths_through_objects_and_arrays() {
        let (_, data) = extract_text("#data c = { \"n\": 3, \"xs\": [10, 20] };\n");
        let n = data
            .resolve(
                "c",
                &[Accessor::Field {
                    name: "n".into(),
                    pos: 0,
                }],
                "s",
                0,
            )
            .unwrap();
        assert_eq!(n, 3.0);
        let x = data
            .resolve(
                "c",
                &[
                    Accessor::Field {
                        name: "xs".into(),
                        pos: 0,
                    },
                    Accessor::Index { index: 1, pos: 0 },
                ],
                "s",
                0,
            )
            .unwrap();
        assert_eq!(x, 20.0);
    }

    #[test]
    fn resolves_a_bare_table_as_a_number() {
        let (_, data) = extract_text("#data n = 7;\n");
        assert_eq!(data.resolve("n", &[], "s", 0).unwrap(), 7.0);
    }

    #[test]
    fn booleans_resolve_to_one_and_zero() {
        let (_, data) = extract_text("#data on = true;\n");
        assert_eq!(data.resolve("on", &[], "s", 0).unwrap(), 1.0);
    }

    #[test]
    fn reports_bad_paths() {
        let (_, data) = extract_text("#data c = { \"xs\": [1], \"s\": \"x\" };\n");
        let e = data
            .resolve(
                "c",
                &[Accessor::Field {
                    name: "nope".into(),
                    pos: 0,
                }],
                "s",
                0,
            )
            .unwrap_err();
        assert!(e.message.contains("has no field `nope`"), "{e}");
        assert!(e.message.contains("fields: xs, s"), "{e}");

        let e = data
            .resolve(
                "c",
                &[
                    Accessor::Field {
                        name: "xs".into(),
                        pos: 0,
                    },
                    Accessor::Index { index: 5, pos: 0 },
                ],
                "s",
                0,
            )
            .unwrap_err();
        assert!(e.message.contains("out of range"), "{e}");

        let e = data
            .resolve(
                "c",
                &[Accessor::Field {
                    name: "s".into(),
                    pos: 0,
                }],
                "s",
                0,
            )
            .unwrap_err();
        assert!(e.message.contains("only numbers and booleans"), "{e}");
    }

    #[test]
    fn rejects_duplicate_and_reserved_names() {
        assert!(
            error("#data a = 1;\n#data a = 2;\n")
                .message
                .contains("already declared")
        );
        assert!(error("#data pi = 1;\n").message.contains("reserved"));
        assert!(
            error("#data tests = 1;\n#tests = [];\n")
                .message
                .contains("already declared")
        );
    }

    #[test]
    fn rejects_malformed_directives() {
        assert!(error("#data a = 1\n").message.contains("expected `;`"));
        assert!(error("#data a 1;\n").message.contains("expected `=`"));
        assert!(error("#data = 1;\n").message.contains("expected a name"));
        assert!(
            error("#data a = 1; oops\n")
                .message
                .contains("unexpected text")
        );
        assert!(
            error("#data a = {oops};\n")
                .message
                .contains("invalid JSON")
        );
    }

    #[test]
    fn ignores_hash_text_inside_comments() {
        let source = "// #data a = 1;\n/*\n#data b = 2;\n*/\nprint(1);\n";
        let (_, data) = extract_text(source);
        assert!(
            data.is_empty(),
            "found {:?}",
            data.names().collect::<Vec<_>>()
        );
    }

    #[test]
    fn other_hash_directives_are_left_for_the_lexer() {
        let (text, data) = extract_text("#mode CMPLX\n#reg x = A\n");
        assert!(data.is_empty());
        assert!(text.contains("#mode CMPLX"));
        assert!(text.contains("#reg x = A"));
    }

    #[test]
    fn reads_a_table_from_a_file() {
        let dir = std::env::temp_dir().join("fx_data_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("values.json");
        std::fs::write(&path, r#"{ "n": 42 }"#).unwrap();
        let (_, data) = extract_text(&format!("#data v = \"{}\";\n", path.display()));
        assert_eq!(
            data.get("v").unwrap().value.get("n"),
            Some(&Json::Number(42.0))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_file_is_a_clear_error() {
        let e = error("#data v = \"definitely-not-here.json\";\n");
        assert!(e.message.contains("cannot read data file"), "{e}");
    }
}
