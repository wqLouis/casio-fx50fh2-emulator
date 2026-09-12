//! Shared helpers for the `.fxc` integration tests.
//!
//! Every program now needs a `fn main()` entry point, so the tests that were
//! written against the old "loose statements at the top level" form wrap their
//! source here. Wrapping does not change the emitted PRGM: `main`'s body is
//! allocated and emitted in statement order, exactly as the old top level was.

/// Wrap `source` in `fn main() { … }`.
///
/// * Leading `#mode`/`#data`/`#tests`/`#include` directives are left at the top
///   level, where the preprocessor expects them.
/// * A source that already defines `main` is returned unchanged.
///
/// The wrapper adds one line before the body (plus one per leading directive),
/// so a test that asserts an exact line number must account for the shift.
pub fn wrap(source: &str) -> String {
    if source.contains("fn main") {
        return source.to_string();
    }

    let mut directives = String::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        let is_directive = trimmed.starts_with('#');
        if !in_body && (is_directive || trimmed.is_empty()) {
            directives.push_str(line);
            directives.push('\n');
        } else {
            in_body = true;
            body.push_str(line);
            body.push('\n');
        }
    }

    format!("{directives}fn main() {{\n{body}}}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_plain_statements() {
        assert_eq!(wrap("print(a);"), "fn main() {\nprint(a);\n}\n");
    }

    #[test]
    fn keeps_directives_at_the_top() {
        assert_eq!(
            wrap("#mode BASE\nprint(a);"),
            "#mode BASE\nfn main() {\nprint(a);\n}\n"
        );
        assert_eq!(
            wrap("#data d = 1;\nprint(d);"),
            "#data d = 1;\nfn main() {\nprint(d);\n}\n"
        );
    }

    #[test]
    fn leaves_an_existing_main_alone() {
        let source = "fn main() { print(1); }";
        assert_eq!(wrap(source), source);
    }

    #[test]
    fn an_empty_source_still_wraps() {
        assert_eq!(wrap(""), "fn main() {\n}\n");
    }
}
