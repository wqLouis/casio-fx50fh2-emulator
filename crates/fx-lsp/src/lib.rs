//! Language-server support for the CASIO fx-50FH II languages.
//!
//! Two source languages are served: the calculator's own PRGM language
//! (`fx`, extension `.fx`) and the C-like language (`fxc`, extension `.fxc`)
//! that the transpiler lowers to PRGM.
//!
//! The crate is split so that all of the interesting behaviour lives in
//! [`logic`], a pure module with no I/O and no async.  [`server`] is a thin
//! stdio JSON-RPC shell around it.  See `README.md` for editor setup.
//!
//! The server is normally launched with `fx50 lsp`; [`run_server`] is the
//! library entry point that the unified CLI calls.

pub mod logic;
pub mod server;

pub use logic::Language;
pub use server::run_server;
