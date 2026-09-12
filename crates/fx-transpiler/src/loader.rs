//! Where the transpiler gets the files it reads.
//!
//! Two language features read files: `#include` pulls in a library of `fn`
//! definitions, and `#data`/`#tests` read a JSON value. Both go through
//! [`FileLoader`] rather than calling `std::fs` directly, because the
//! transpiler also runs where there is no filesystem — in a browser (behind
//! `fx-wasm`) and against an editor's unsaved buffers.
//!
//! The default is [`FsLoader`], so anything that used to read the disk reads it
//! exactly as before; [`MemoryLoader`] is what a host with its own idea of
//! "files" passes in.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Read the files the transpiler is told to read.
///
/// The error is a human-readable reason rather than a platform error type, so
/// that an in-memory loader can say something as useful as the filesystem does.
pub trait FileLoader {
    /// Read `path` as UTF-8 text.
    fn read(&self, path: &Path) -> Result<String, String>;
}

/// The real filesystem: the default, and what the CLI uses.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsLoader;

impl FileLoader for FsLoader {
    fn read(&self, path: &Path) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    }
}

/// Files held in memory, keyed by path.
///
/// Lookups are deliberately forgiving, because the keys come from a user
/// interface rather than from a filesystem. `.` and `..` are resolved and a
/// leading `/` is ignored on either side, so `lib/pack.fxc`, `./lib/pack.fxc`
/// and `/lib/pack.fxc` all find the same entry. That matters for a browser: the
/// root document may be keyed `main.fxc`, `/main.fxc` or `src/main.fxc`
/// depending on how the site names its files, and `#include "lib/pack.fxc"`
/// has to work for all of them.
///
/// Resolution stays *lexical* — no filesystem is consulted, so two spellings of
/// the same path always agree. Include cycle detection depends on that.
#[derive(Debug, Clone, Default)]
pub struct MemoryLoader {
    files: BTreeMap<PathBuf, String>,
}

impl MemoryLoader {
    /// An empty set of files.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file, replacing any earlier text stored under the same path.
    pub fn insert(&mut self, path: impl AsRef<Path>, text: impl Into<String>) {
        self.files.insert(normalize(path.as_ref()), text.into());
    }

    /// Whether `path` names a file here, by the same forgiving rules as
    /// [`FileLoader::read`].
    pub fn contains(&self, path: &Path) -> bool {
        self.lookup(path).is_some()
    }

    /// How many files were provided.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether no files were provided.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The paths, normalised, in sorted order.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(PathBuf::as_path)
    }

    fn lookup(&self, path: &Path) -> Option<&String> {
        let wanted = normalize(path);
        if let Some(text) = self.files.get(&wanted) {
            return Some(text);
        }
        // Tolerate the two sides disagreeing about a leading root: compare what
        // is left once `/`, `.` and `..` are dropped. This is the only
        // difference a user interface realistically introduces, and it keeps a
        // wrong guess from matching a *different* file, which comparing bare
        // file names would not.
        let bare = strip_root(&wanted);
        if bare.as_os_str().is_empty() {
            return None;
        }
        self.files
            .iter()
            .find(|(key, _)| strip_root(key) == bare)
            .map(|(_, text)| text)
    }
}

impl FileLoader for MemoryLoader {
    fn read(&self, path: &Path) -> Result<String, String> {
        match self.lookup(path) {
            Some(text) => Ok(text.clone()),
            None => Err(format!("no source named `{}` was provided", path.display())),
        }
    }
}

/// Resolve `.` and `..` lexically, without touching the filesystem.
///
/// `canonicalize` would be the obvious tool, but it needs a real file, so it
/// cannot be used for in-memory sources and it resolves symlinks differently on
/// different machines. Cycle detection wants the opposite: the same answer
/// everywhere, from the text of the path alone.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Step out of a real directory name; otherwise keep the `..`
                // (at the start of a relative path) or drop it (at a root).
                if out.file_name().is_some() {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Everything after the root: `Normal` components only.
fn strip_root(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolves_dots() {
        assert_eq!(normalize(Path::new("./a/b.fxc")), PathBuf::from("a/b.fxc"));
        assert_eq!(normalize(Path::new("a/./b.fxc")), PathBuf::from("a/b.fxc"));
        assert_eq!(
            normalize(Path::new("a/x/../b.fxc")),
            PathBuf::from("a/b.fxc")
        );
        assert_eq!(normalize(Path::new(".")), PathBuf::new());
        assert_eq!(normalize(Path::new("/a/b")), PathBuf::from("/a/b"));
        assert_eq!(normalize(Path::new("/a/../b")), PathBuf::from("/b"));
    }

    #[test]
    fn normalize_keeps_leading_parents() {
        // A relative path may legitimately start by climbing.
        assert_eq!(normalize(Path::new("../a.fxc")), PathBuf::from("../a.fxc"));
        assert_eq!(
            normalize(Path::new("../../a.fxc")),
            PathBuf::from("../../a.fxc")
        );
        // …but never climbs out of a root.
        assert_eq!(normalize(Path::new("/../a.fxc")), PathBuf::from("/a.fxc"));
    }

    #[test]
    fn memory_loader_finds_the_same_file_by_several_spellings() {
        let mut files = MemoryLoader::new();
        files.insert("lib/pack.fxc", "fn pack(x, y) = x + y * i();");
        for spelling in [
            "lib/pack.fxc",
            "./lib/pack.fxc",
            "/lib/pack.fxc",
            "lib/./pack.fxc",
            "src/../lib/pack.fxc",
        ] {
            assert!(
                files.contains(Path::new(spelling)),
                "`{spelling}` should find `lib/pack.fxc`"
            );
            assert_eq!(
                files.read(Path::new(spelling)).unwrap(),
                "fn pack(x, y) = x + y * i();"
            );
        }
    }

    #[test]
    fn memory_loader_does_not_confuse_different_files() {
        let mut files = MemoryLoader::new();
        files.insert("a/pack.fxc", "first");
        files.insert("b/pack.fxc", "second");
        // A bare name is ambiguous, so it must not silently pick one.
        assert_eq!(files.read(Path::new("a/pack.fxc")).unwrap(), "first");
        assert_eq!(files.read(Path::new("b/pack.fxc")).unwrap(), "second");
        assert!(!files.contains(Path::new("c/pack.fxc")));
    }

    #[test]
    fn a_missing_file_says_so_without_a_platform_error() {
        let files = MemoryLoader::new();
        let error = files.read(Path::new("nope.fxc")).unwrap_err();
        assert!(error.contains("no source named `nope.fxc`"), "{error}");
    }

    #[test]
    fn inserting_the_same_path_twice_replaces_it() {
        let mut files = MemoryLoader::new();
        files.insert("./a.fxc", "one");
        files.insert("a.fxc", "two");
        assert_eq!(files.len(), 1);
        assert_eq!(files.read(Path::new("a.fxc")).unwrap(), "two");
    }

    #[test]
    fn fs_loader_reports_a_missing_file() {
        // The error text differs from `MemoryLoader`'s, which is the point of
        // returning a string rather than an `io::Error`.
        let error = FsLoader
            .read(Path::new("/definitely/not/here.fxc"))
            .unwrap_err();
        assert!(!error.is_empty());
    }
}
