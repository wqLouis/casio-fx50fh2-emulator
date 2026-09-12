//! `#include` preprocessing: inline another file at transpile time.
//!
//! ```text
//! // main.fxc
//! #include "lib/geometry.fxc"
//!
//! fn main() {
//!     print(hypot(3, 4));
//! }
//! ```
//!
//! This is the `.fxc` analogue of C's `#include` or Rust's `include_str!`: the
//! referenced file's text is substituted in place, before lexing, so the
//! transpiler sees one flat program. The include itself never reaches the
//! calculator — only the expanded program does.
//!
//! Rules:
//!
//! * The directive is `#include "path"`, and must be the first thing on the
//!   line apart from leading whitespace. An optional `// comment` may follow.
//! * The path is resolved **relative to the file containing the directive**,
//!   which is what makes a library relocatable.
//! * **An include is a top-level directive.** It contributes *declarations* —
//!   `fn` definitions and compile-time `const`/`#data` values — never statements
//!   spliced into a body. An `#include` inside a `fn` body is an error; see
//!   below.
//! * Includes nest, up to [`MAX_DEPTH`], and a cycle is reported rather than
//!   recursing forever.
//! * Expansion is purely textual and unguarded, exactly like C: including the
//!   same file twice includes its text twice.
//! * `#mode` may only appear in the root file. An included file that declares a
//!   mode is an error, because the mode applies to the whole program.
//!
//! ## Libraries, not fragments
//!
//! An included file is a **library**: a set of `fn` definitions (and optionally
//! `const`/`#data` values) that the including program calls. It has no `fn
//! main()` of its own, and building it directly is not an error — it simply
//! produces an empty program, because there is nothing to run.
//!
//! Earlier versions also allowed a *statement fragment*: a file of loose
//! statements spliced into whatever included it, sharing the includer's
//! variables. That is gone. It made a file's meaning depend on where it was
//! included, let an include silently introduce or clobber the includer's
//! memories, and gave a buildable file no way to say what it needed. A library
//! takes parameters and returns values, so its dependencies are in its signature
//! rather than in its caller's memory layout.
//!
//! Because expansion rewrites the source, byte offsets no longer point into
//! any real file. [`Expanded`] carries a line map so a diagnostic can be
//! reported against the file and line that actually caused it.

use std::path::{Path, PathBuf};

use crate::error::TranspileError;
use crate::loader::{FileLoader, normalize};

/// How deep `#include` may nest before we assume something is wrong.
const MAX_DEPTH: usize = 32;

/// Source after include expansion, plus the map back to the original files.
#[derive(Debug, Clone)]
pub(crate) struct Expanded {
    /// The concatenated source that gets lexed.
    pub(crate) text: String,
    /// For each line of [`Expanded::text`], the file it came from and its
    /// 1-based line number within that file.
    line_origins: Vec<(usize, usize)>,
    /// Files referenced by `line_origins`. `None` is an anonymous root, used
    /// when transpiling a string that came from nowhere in particular.
    files: Vec<Option<PathBuf>>,
}

impl Expanded {
    /// The file and 1-based line that line `line` (1-based) of
    /// [`Expanded::text`] came from.
    pub(crate) fn origin(&self, line: usize) -> (Option<&Path>, usize) {
        match self.line_origins.get(line.saturating_sub(1)) {
            Some((file, file_line)) => {
                (self.files.get(*file).and_then(Option::as_deref), *file_line)
            }
            None => (None, line),
        }
    }
}

/// Like [`expand`], but reads included files through `loader`.
pub(crate) fn expand_with(
    source: &str,
    root: Option<&Path>,
    base_dir: &Path,
    loader: &dyn FileLoader,
) -> Result<Expanded, TranspileError> {
    let mut expander = Expander::new(loader);
    let root_index = expander.file_index(root.map(Path::to_path_buf));
    expander.root = root.map(Path::to_path_buf);
    expander.base_dir = base_dir.to_path_buf();
    if let Some(path) = root {
        expander.stack.push(normalize(path));
    }

    expander.walk(
        source,
        root_index,
        root.map(Path::to_path_buf).as_deref(),
        true,
    )?;

    Ok(Expanded {
        text: expander.text,
        line_origins: expander.line_origins,
        files: expander.files,
    })
}

struct Expander<'a> {
    text: String,
    line_origins: Vec<(usize, usize)>,
    files: Vec<Option<PathBuf>>,
    /// Files currently being expanded, for cycle detection.
    stack: Vec<PathBuf>,
    root: Option<PathBuf>,
    base_dir: PathBuf,
    /// How included files are read.
    loader: &'a dyn FileLoader,
}

impl<'a> Expander<'a> {
    fn new(loader: &'a dyn FileLoader) -> Self {
        Expander {
            text: String::new(),
            line_origins: Vec::new(),
            files: Vec::new(),
            stack: Vec::new(),
            root: None,
            base_dir: PathBuf::new(),
            loader,
        }
    }
}

impl Expander<'_> {
    fn file_index(&mut self, path: Option<PathBuf>) -> usize {
        if let Some(existing) = self.files.iter().position(|known| *known == path) {
            return existing;
        }
        self.files.push(path);
        self.files.len() - 1
    }

    /// Walk one file's text, appending lines and recursing on `#include`.
    ///
    /// `at` is the path of the file being walked (used to resolve nested
    /// includes); it is `None` for an anonymous root, which resolves against
    /// [`Expander::base_dir`] instead.
    fn walk(
        &mut self,
        source: &str,
        file_index: usize,
        at: Option<&Path>,
        is_root: bool,
    ) -> Result<(), TranspileError> {
        let dir = at
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.base_dir.clone());

        // Brace depth of the *code* seen so far in this file, used to reject an
        // `#include` that sits inside a function body. Braces in comments and
        // string literals do not count, and braces inside a `#data`/`#tests`
        // value balance out because the value is valid JSON.
        let mut depth = 0i32;
        let mut in_block_comment = false;

        for (index, line) in source.lines().enumerate() {
            let line_number = index + 1;

            if let Some(relative) = parse_include(line) {
                if depth > 0 {
                    return Err(TranspileError::at(
                        source,
                        "`#include` must be at the top level, not inside a function body: \
                         a library contributes `fn` definitions to be called, not statements \
                         spliced into a body",
                        line_offset(source, line_number),
                    )
                    .in_file(at));
                }
                self.include(&relative, &dir, source, line_number, at)?;
                continue;
            }

            if !is_root && is_mode_directive(line) {
                return Err(TranspileError::at(
                    source,
                    "`#mode` may only appear in the program's own file, not in an included file",
                    line_offset(source, line_number),
                )
                .in_file(at));
            }

            depth += code_brace_delta(line, &mut in_block_comment);

            self.text.push_str(line);
            self.text.push('\n');
            self.line_origins.push((file_index, line_number));
        }
        Ok(())
    }

    /// Expand one `#include`.
    ///
    /// `source`/`line_number` locate the directive, and `at` names the file it
    /// appears in, so a failure points at the offending `#include`.
    fn include(
        &mut self,
        relative: &str,
        dir: &Path,
        source: &str,
        line_number: usize,
        at: Option<&Path>,
    ) -> Result<(), TranspileError> {
        let target = dir.join(relative);
        let canonical_target = normalize(&target);
        let offset = line_offset(source, line_number);

        if self.stack.contains(&canonical_target) {
            let chain: Vec<String> = self
                .stack
                .iter()
                .map(|p| p.display().to_string())
                .chain(std::iter::once(canonical_target.display().to_string()))
                .collect();
            return Err(TranspileError::at(
                source,
                format!("circular `#include`: {}", chain.join(" → ")),
                offset,
            )
            .in_file(at));
        }
        if self.stack.len() >= MAX_DEPTH {
            return Err(TranspileError::at(
                source,
                format!("`#include` nested more than {MAX_DEPTH} deep"),
                offset,
            )
            .in_file(at));
        }

        let included = self.loader.read(&target).map_err(|e| {
            TranspileError::at(
                source,
                format!("cannot include `{}`: {e}", target.display()),
                offset,
            )
            .in_file(at)
        })?;

        let index = self.file_index(Some(target.clone()));
        self.stack.push(canonical_target);
        let result = self.walk(&included, index, Some(&target), false);
        self.stack.pop();
        result
    }
}

/// The net change in brace depth contributed by one line of **code**.
///
/// Used only to decide whether an `#include` is inside a function body, so it
/// errs towards *not* counting a brace: anything inside a `//` comment, a
/// `/* … */` comment or a string literal is skipped, and `*in_block_comment`
/// carries the block-comment state across lines.
///
/// Braces inside a `#data`/`#tests` JSON value are counted, which is fine: the
/// value is valid JSON, so they balance out before the directive ends and an
/// `#include` cannot appear inside it.
fn code_brace_delta(line: &str, in_block_comment: &mut bool) -> i32 {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut delta = 0i32;
    let mut in_string = false;

    while i < bytes.len() {
        let byte = bytes[i];
        if *in_block_comment {
            if byte == b'*' && bytes.get(i + 1) == Some(&b'/') {
                *in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if in_string {
            match byte {
                // Skip the escaped character, but not past the end of the line.
                b'\\' => i += 2,
                b'"' => {
                    in_string = false;
                    i += 1;
                }
                _ => i += 1,
            }
            continue;
        }
        match byte {
            b'/' if bytes.get(i + 1) == Some(&b'/') => break,
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                *in_block_comment = true;
                i += 2;
            }
            b'"' => {
                in_string = true;
                i += 1;
            }
            b'{' => {
                delta += 1;
                i += 1;
            }
            b'}' => {
                delta -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    delta
}

/// The byte offset of the start of `line` (1-based) in `source`.
fn line_offset(source: &str, line: usize) -> usize {
    let mut offset = 0;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        if index + 1 == line {
            return offset;
        }
        offset += text.len();
    }
    source.len()
}

/// Recognise `#include "path"` and return the path.
///
/// Only the first thing on the line counts, matching C's rules, so `#include`
/// appearing inside an expression is left for the lexer to reject.
fn parse_include(line: &str) -> Option<String> {
    let rest = line.trim_start();
    let rest = rest.strip_prefix("#include")?;
    // The directive name must be followed by whitespace, not more letters.
    if !rest.starts_with([' ', '\t']) {
        return None;
    }
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let (path, rest) = rest.split_once('"')?;
    if path.is_empty() {
        return None;
    }
    // Only whitespace or a line comment may follow.
    let rest = rest.trim();
    if rest.is_empty() || rest.starts_with("//") {
        Some(path.to_string())
    } else {
        None
    }
}

/// Is this line a `#mode` directive?
fn is_mode_directive(line: &str) -> bool {
    let rest = line.trim_start();
    match rest.strip_prefix("#mode") {
        Some(after) => after.is_empty() || after.starts_with([' ', '\t', '=']),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_a_plain_include() {
        assert_eq!(parse_include("#include \"a.fxc\""), Some("a.fxc".into()));
        assert_eq!(
            parse_include("  #include \"dir/b.fxc\""),
            Some("dir/b.fxc".into())
        );
        assert_eq!(
            parse_include("#include \"a.fxc\" // why"),
            Some("a.fxc".into())
        );
    }

    #[test]
    fn ignores_things_that_are_not_includes() {
        assert_eq!(parse_include("let a = 1;"), None);
        assert_eq!(parse_include("#including \"a\""), None);
        assert_eq!(parse_include("#include"), None);
        assert_eq!(parse_include("#include a.fxc"), None);
        assert_eq!(parse_include("#include \"\""), None);
        assert_eq!(parse_include("#include \"a.fxc\" garbage"), None);
        // `#mode` is not an include.
        assert_eq!(parse_include("#mode CMPLX"), None);
    }

    #[test]
    fn detects_mode_directives() {
        assert!(is_mode_directive("#mode CMPLX"));
        assert!(is_mode_directive("  #mode=cmplx"));
        assert!(!is_mode_directive("#modex"));
        assert!(!is_mode_directive("#include \"a\""));
    }

    #[test]
    fn expansion_without_includes_is_identity() {
        let expanded = expand_with(
            "let a = 1;\nprint(a);\n",
            None,
            Path::new("."),
            &crate::loader::FsLoader,
        )
        .unwrap();
        assert_eq!(expanded.text, "let a = 1;\nprint(a);\n");
        assert_eq!(expanded.files.len(), 1, "no included file was added");
    }

    #[test]
    fn line_offset_finds_each_line() {
        let source = "abc\ndefg\nhi";
        assert_eq!(line_offset(source, 1), 0);
        assert_eq!(line_offset(source, 2), 4);
        assert_eq!(line_offset(source, 3), 9);
    }
}
