//! Zed extension for the CASIO fx-50FH II.
//!
//! The extension provides syntax highlighting for the two language variants
//! (`fx`, the on-calculator PRGM language, and `fxc`, its C-like transpiler
//! source) and starts the `fx50` language server with the `lsp` subcommand.
//!
//! API notes (verified against the `zed_extension_api` 0.1.0 docs on docs.rs,
//! the version required by `Cargo.toml`):
//!
//! * `Extension::language_server_command` is `&mut self` and returns
//!   `Result<Command>`.
//! * `Command` is a struct with `command: String`, `args: Vec<String>` and
//!   `env: EnvVars`, where `EnvVars = Vec<(String, String)>`.
//! * `Worktree::root_path()` returns a `String` and `Worktree::which(name)`
//!   returns `Option<String>`.
//! * `register_extension!` takes the extension type.

use std::path::Path;

use zed_extension_api as zed;
use zed_extension_api::{Command, LanguageServerId, Result, Worktree};

/// The fx-50FH II Zed extension.
struct Fx50Extension;

impl zed::Extension for Fx50Extension {
    fn new() -> Self {
        Fx50Extension
    }

    /// Start the language server as `fx50 lsp`.
    ///
    /// The executable is resolved in this order:
    ///
    /// 1. a binary built inside the worktree, at `target/release/fx50` or
    ///    `target/debug/fx50` (so a plain `cargo build` just works);
    /// 2. `fx50` somewhere on `$PATH` (via [`Worktree::which`]);
    /// 3. the bare name `fx50`, letting the OS resolve it.
    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let command = resolve_language_server(worktree).ok_or_else(|| {
            "could not find the `fx50` language server. Build it from the \
             repository root with `cargo build --release` (the extension looks \
             for `target/release/fx50` or `target/debug/fx50`), or install it \
             so that it is on your PATH."
                .to_string()
        })?;

        // `language_server_id` is currently unused: there is exactly one
        // server, launched for both the `fx` and `fxc` languages.
        let _ = language_server_id;

        Ok(Command {
            command,
            args: vec!["lsp".to_string()],
            env: vec![],
        })
    }
}

/// Locate the `fx50` executable, preferring a binary built in the worktree.
fn resolve_language_server(worktree: &Worktree) -> Option<String> {
    let root = worktree.root_path();

    for relative in ["target/release/fx50", "target/debug/fx50"] {
        for candidate in [relative.to_string(), format!("{relative}.exe")] {
            let path = Path::new(&root).join(candidate);
            if path.is_file() {
                return Some(path.to_string_lossy().into_owned());
            }
        }
    }

    if let Some(path) = worktree.which("fx50") {
        return Some(path);
    }

    Some("fx50".to_string())
}

zed::register_extension!(Fx50Extension);
