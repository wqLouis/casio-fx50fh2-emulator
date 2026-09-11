# CASIO fx-50FH II — VS Code extension

Language support for the CASIO fx-50FH II programmable calculator, powered by
the `fx50 lsp` language server from this repository.

It contributes two languages:

| Language id | Files | Description |
| --- | --- | --- |
| `fx` | `*.fx` | The calculator's native **PRGM** keystroke language. |
| `fxc` | `*.fxc` | The C-like source language that transpiles to PRGM. |

Both are served by the same `fx50 lsp` process. The server dispatches on the
`languageId` the client sends, so this extension declares exactly these two ids
and selects the server for both. Features are diagnostics, completion, hover
and document symbols.

## Prerequisites

**The extension cannot ship the server, so `fx50` must be installed on your
system first.** (A VS Code extension is a zip of JavaScript; it has nowhere to
put a platform executable.) The extension then *finds* it — see
[How the server path is resolved](#how-the-server-path-is-resolved).

The easiest way is [`cargo install`](https://doc.rust-lang.org/cargo/commands/cargo-install.html),
which puts `fx50` in `~/.cargo/bin`:

```bash
# Straight from GitHub, no clone needed:
cargo install --git https://github.com/wqLouis/casio-fx50fh2-emulator fx-cli

# Or from a clone:
cargo install --path crates/fx-cli
```

If you are working in this repository instead, `cargo build` is enough — the
extension also looks for `target/debug/fx50` in the workspace folder. Confirm
the binary works with `fx50 --version`.

## Build the extension

```bash
cd editors/vscode
npm install
npm run compile
```

`npm run compile` runs `tsc -p .` and writes `out/extension.js`.

## Run it

* **Develop / try it out:** open this folder in VS Code and press **F5**
  ("Run Extension"). A second *Extension Development Host* window opens with
  the extension active. Open any `examples/*.fx` or `examples/*.fxc` file from
  the repository.
* **Package a `.vsix`:** install [`@vscode/vsce`](https://github.com/microsoft/vscode-vsce)
  and run:

  ```bash
  npx @vscode/vsce package
  ```

  then install the resulting `.vsix` with *Extensions: Install from VSIX…*.

You can also reload or restart the server at any time with the
**fx-50FH II: Restart Language Server** command (`fx50.restartServer`).

## How the server path is resolved

When the extension activates it looks for the `fx50` executable in this order:

1. The `fx50.serverPath` setting, if it is non-empty. Use this to pin an exact
   binary, e.g. `"fx50.serverPath": "/home/me/casio-fx50fh2/target/debug/fx50"`.
2. `fx50` found on `PATH` (a synchronous directory scan, not a shell call).
3. Inside each open workspace folder, the first existing of:
   * `<folder>/target/release/fx50`
   * `<folder>/target/debug/fx50`

If none is found, the extension shows an error telling you to run `cargo build`
or set `fx50.serverPath`. Setting `fx50.serverPath` overrides everything else.

The trace level is controlled by `fx50.trace.server` (`off`, `messages` or
`verbose`); trace output goes to the **CASIO fx-50FH II** Output channel.

## Settings

| Setting | Default | Meaning |
| --- | --- | --- |
| `fx50.serverPath` | `""` | Absolute path to the `fx50` binary. Empty means auto-discover. |
| `fx50.trace.server` | `"off"` | LSP trace level: `off`, `messages` or `verbose`. |

## Layout

```
editors/vscode/
├── package.json                     extension manifest
├── language-configuration.json      fx (PRGM) editor configuration
├── language-configuration-fxc.json  fxc editor configuration
├── syntaxes/
│   ├── fx.tmLanguage.json           TextMate grammar, scopeName source.fx
│   └── fxc.tmLanguage.json          TextMate grammar, scopeName source.fxc
├── src/extension.ts                 the language client
├── tsconfig.json
├── .vscodeignore
├── .gitignore
├── README.md
└── CHANGELOG.md
```
