//! Standalone `fx-lsp` binary.
//!
//! The real entry point is `fx50 lsp`; this shim exists so the crate produces
//! a runnable stdio server on its own (used by the end-to-end test and by
//! editors that expect a dedicated binary).

fn main() {
    if let Err(err) = fx_lsp::run_server() {
        eprintln!("fx-lsp: {err}");
        std::process::exit(1);
    }
}
