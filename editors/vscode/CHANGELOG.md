# Changelog

All notable changes to the CASIO fx-50FH II VS Code extension are documented
here. The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2025-01-01

### Added

- Language support for `fx` (PRGM `.fx`) and `fxc` (C-like `.fxc`), launched
  from the `fx50 lsp` language server over stdio.
- Diagnostics, completion, hover and document-symbol support via
  `vscode-languageclient`.
- TextMate grammars for both languages (`source.fx`, `source.fxc`).
- Language configurations for both languages (comments, brackets,
  auto-closing pairs and word patterns).
- `fx50.serverPath` and `fx50.trace.server` settings, with server discovery
  from the setting, `PATH`, then the workspace `target/` directory.
- `fx50.restartServer` command to restart the language server.

[0.1.0]: https://example.invalid/casio-fx50fh2/editors/vscode/CHANGELOG.md
