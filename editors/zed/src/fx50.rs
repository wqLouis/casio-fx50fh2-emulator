//! Zed extension for the CASIO fx-50FH II.
//!
//! It provides syntax highlighting for the two language variants — `fx`, the
//! on-calculator PRGM language, and `fxc`, its C-like source — and starts the
//! `fx50` language server with the `lsp` subcommand.
//!
//! # Finding the binary
//!
//! A wasm extension cannot stat the filesystem; the [`Worktree`] API exposes
//! only `which`, `root_path`, `read_text_file` and `shell_env`. So the binary
//! is resolved in this order:
//!
//! 1. `fx50` on `$PATH` (via [`Worktree::which`]) — the case after
//!    `cargo install --path crates/fx-cli` or a symlink into a `bin` dir.
//! 2. A build inside the opened worktree: the dep-info file Cargo writes next
//!    to the binary (`target/release/fx50.d` or `target/debug/fx50.d`) is
//!    probed with [`Worktree::read_text_file`]. That method requires a path
//!    *relative to the worktree* (an absolute path fails with "absolute path
//!    not allowed") and only reads text, so the dep-info file — not the
//!    binary — is what gets read.
//! 3. The worktree is this repository. `target/` is gitignored, and the
//!    worktree file API cannot see ignored paths, so step 2 never fires here.
//!    Instead the root `Cargo.toml` (tracked, therefore readable) is checked
//!    for [`WORKSPACE_MARKER`]; if it matches, the documented
//!    `cargo build` layout is assumed.
//! 4. Otherwise a clear error telling the user how to make the binary
//!    findable.
//!
//! API notes (verified against `zed_extension_api` 0.1.0, Zed's `HostWorktree`
//! implementation, and the `since_v0.8.0` WIT definition):
//!
//! * `Extension::language_server_command` takes `&mut self`.
//! * `Command` has `command: String`, `args: Vec<String>` and
//!   `env: EnvVars` (`Vec<(String, String)>`).
//! * `Worktree::root_path` returns a `String`; `which` returns
//!   `Option<String>`; `read_text_file` returns `Result<String, String>`.
//! * The `worktree` resource has only `id`, `root-path`, `read-text-file`,
//!   `which` and `shell-env`. There is no way to test existence of a binary,
//!   and no way to read a gitignored path.

use zed_extension_api as zed;
use zed_extension_api::{Command, LanguageServerId, Result, Worktree};

/// The fx-50FH II Zed extension.
struct Fx50Extension;

/// Where a built binary may sit inside a worktree, most preferred first.
///
/// The first element is the dep-info file Cargo writes next to the binary and
/// the second is the binary itself. Cargo's dep-info file is plain text, so it
/// is the one thing a wasm extension can read to tell whether that build
/// profile produced a binary — `read_text_file` cannot read the ELF executable,
/// and the API has no `stat`/`exists` (even `v0.8.0` only offers
/// `read-text-file` and `which`).
///
/// This only works when `target/` is visible to the worktree. It is normally
/// gitignored, in which case the worktree file API cannot see it at all — see
/// [`WORKSPACE_MARKER`].
const WORKTREE_BUILDS: [(&str, &str); 2] = [
    ("target/release/fx50.d", "target/release/fx50"),
    ("target/debug/fx50.d", "target/debug/fx50"),
];

/// A string that appears in this repository's root `Cargo.toml`.
///
/// `Cargo.toml` is tracked, so it is always readable, unlike `target/`. Its
/// presence identifies the fx50 repository, whose layout we then know: a plain
/// `cargo build` puts the binary at `target/debug/fx50`.
const WORKSPACE_MARKER: &str = "crates/fx-cli";

const NOT_FOUND: &str = "could not find the `fx50` language server. Either build it in this \
     worktree with `cargo build` (so that `target/debug/fx50` exists), or put `fx50` on your \
     PATH (`cargo install --path crates/fx-cli`, or a symlink to the binary in a `bin` \
     directory).";

impl zed::Extension for Fx50Extension {
    fn new() -> Self {
        Fx50Extension
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        // There is exactly one server, launched for both `fx` and `fxc`.
        let _ = language_server_id;

        Ok(Command {
            command: resolve_language_server(worktree)?,
            args: vec!["lsp".to_string()],
            env: vec![],
        })
    }
}

/// Locate the `fx50` executable. See the module docs for the order.
fn resolve_language_server(worktree: &Worktree) -> Result<String> {
    // 1. On `$PATH`.
    if let Some(path) = worktree.which("fx50") {
        return Ok(path);
    }

    // 2. Built inside the worktree. Paths here are worktree-relative —
    //    `read_text_file` rejects an absolute path — while the returned
    //    command path is absolute so it spawns regardless of the server's
    //    working directory.
    let root = worktree.root_path();
    for (marker, binary) in WORKTREE_BUILDS {
        if worktree.read_text_file(marker).is_ok() {
            return Ok(format!("{root}/{binary}"));
        }
    }

    // 3. The worktree is the fx50 repository itself. `target/` is gitignored,
    //    so step 2 cannot see it; detect the repository by a tracked file
    //    instead and trust the documented layout (`cargo build` -> debug).
    if is_fx50_workspace(worktree) {
        return Ok(format!("{root}/target/debug/fx50"));
    }

    // 4. Nothing found: say so, rather than spawning a path that cannot work.
    Err(NOT_FOUND.to_string())
}

/// Does this worktree look like the fx50 repository?
fn is_fx50_workspace(worktree: &Worktree) -> bool {
    worktree
        .read_text_file("Cargo.toml")
        .is_ok_and(|manifest| manifest.contains(WORKSPACE_MARKER))
}

zed::register_extension!(Fx50Extension);
