//! Repo-consistency checks for the editor integrations in `editors/`.
//!
//! These are not language-server tests. They guard the *wiring* between the
//! three places that have to agree for editor highlighting to work:
//!
//! * the tree-sitter grammars (`editors/tree-sitter-{fx,fxc}`),
//! * the Zed extension that points at them (`editors/zed`),
//! * and the fact that Zed reads `highlights.scm` from the **language**
//!   directory rather than from the grammar checkout.
//!
//! That last point is the subtle one: the queries are duplicated, and if the
//! copies drift, highlighting silently changes for one editor and not the
//! other. A plain string comparison catches it.

use std::path::{Path, PathBuf};

/// The repository root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repository root")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The query body, without its leading `;` comment header.
///
/// The two copies are allowed to differ in their explanatory header (each
/// points at the other), but the patterns themselves must match exactly.
fn query_body(text: &str) -> String {
    let mut lines = text.lines();
    // Skip the leading comment block and the blank line after it.
    let body: Vec<&str> = lines
        .by_ref()
        .skip_while(|line| line.starts_with(';') || line.trim().is_empty())
        .collect();
    body.join("\n").trim().to_string()
}

const LANGUAGES: [&str; 2] = ["fx", "fxc"];

#[test]
fn the_two_highlights_copies_are_kept_in_sync() {
    let root = repo_root();
    for lang in LANGUAGES {
        let zed = read(&root.join(format!("editors/zed/languages/{lang}/highlights.scm")));
        let grammar =
            read(&root.join(format!("editors/tree-sitter-{lang}/queries/highlights.scm")));
        let (a, b) = (query_body(&zed), query_body(&grammar));
        assert!(!a.is_empty(), "{lang}: empty highlight query");
        assert_eq!(
            a, b,
            "editors/zed/languages/{lang}/highlights.scm and \
             editors/tree-sitter-{lang}/queries/highlights.scm have drifted; Zed reads the \
             former, other editors read the latter, so they must stay identical"
        );
    }
}

/// Zed looks for the query next to `config.toml`; if it is not there, the
/// grammar still parses but nothing is highlighted.
#[test]
fn each_zed_language_has_the_files_zed_reads() {
    let root = repo_root();
    for lang in LANGUAGES {
        let dir = root.join(format!("editors/zed/languages/{lang}"));
        for required in ["config.toml", "highlights.scm"] {
            let path = dir.join(required);
            assert!(
                path.is_file(),
                "{} is missing; Zed will not highlight {lang} without it",
                path.display()
            );
        }
    }
}

/// Every language `name` in a `languages/*/config.toml` must be listed by the
/// language server in `extension.toml`, because Zed matches those names
/// verbatim.
#[test]
fn the_zed_manifest_lists_every_configured_language() {
    let root = repo_root();
    let manifest = read(&root.join("editors/zed/extension.toml"));
    for lang in LANGUAGES {
        let config = read(&root.join(format!("editors/zed/languages/{lang}/config.toml")));
        let name = config
            .lines()
            .find_map(|line| line.strip_prefix("name ="))
            .map(|value| value.trim().trim_matches('"').to_string())
            .unwrap_or_else(|| panic!("{lang}/config.toml has no `name`"));

        assert!(
            manifest.contains(&format!("\"{name}\"")),
            "extension.toml does not list language `{name}` (from {lang}/config.toml); \
             Zed matches this name exactly"
        );
        // ...and it must map it back to the id the server expects.
        assert!(
            manifest.contains(&format!("\"{name}\" = \"{lang}\"")),
            "extension.toml has no `language_ids` entry mapping `{name}` to `{lang}`"
        );
    }
}

/// The grammar revision must be a real commit, not a placeholder: Zed clones
/// the repository at that revision to build the parser.
#[test]
fn the_grammar_revisions_are_real_commits() {
    let manifest = read(&repo_root().join("editors/zed/extension.toml"));
    let mut seen = 0;
    for line in manifest.lines() {
        let Some(rev) = line.trim().strip_prefix("rev =") else {
            continue;
        };
        let rev = rev.trim().trim_matches('"');
        seen += 1;
        assert_eq!(rev.len(), 40, "`{rev}` is not a full SHA");
        assert!(
            rev.chars().all(|c| c.is_ascii_hexdigit()),
            "`{rev}` is not hexadecimal"
        );
    }
    assert_eq!(seen, 2, "expected one `rev` per grammar in extension.toml");
}

/// Each grammar must ship the generated parser, because that is what Zed
/// compiles with clang.
#[test]
fn each_grammar_ships_its_generated_parser() {
    let root = repo_root();
    for lang in LANGUAGES {
        let parser = root.join(format!("editors/tree-sitter-{lang}/src/parser.c"));
        assert!(parser.is_file(), "{} is missing", parser.display());
        let source = read(&parser);
        assert!(
            source.contains(&format!("tree_sitter_{lang}")),
            "{} does not define the `tree_sitter_{lang}` entry point",
            parser.display()
        );
    }
}
