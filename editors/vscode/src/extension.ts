// VS Code client for the `fx50 lsp` language server.
//
// The server is the unified `fx50` binary built from this repository; it is
// started as `<fx50-path> lsp` and speaks LSP over stdio. It routes requests by
// the `languageId` the client supplies, so both language ids (`fx` for PRGM
// files and `fxc` for C-like files) are pointed at the same server here.

import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Trace,
} from "vscode-languageclient/node";

/** The two language ids declared in `package.json`. */
const LANGUAGE_IDS = ["fx", "fxc"];

/** The single client instance, or `undefined` before activation. */
let client: LanguageClient | undefined;

/** Diagnostic/trace output shown in the Output panel. */
let outputChannel: vscode.OutputChannel;

/**
 * Whether `candidate` is an existing regular file.
 *
 * Used instead of probing with a shell so discovery stays synchronous and
 * dependency-free.
 */
function isFile(candidate: string): boolean {
  try {
    return fs.statSync(candidate).isFile();
  } catch {
    return false;
  }
}

/**
 * Names to look for on `PATH`.
 *
 * Windows resolves executables with a `.exe` suffix, so `fx50.exe` is tried
 * first there; elsewhere the bare name is the natural first choice.
 */
function executableNames(): string[] {
  return process.platform === "win32" ? ["fx50.exe", "fx50"] : ["fx50", "fx50.exe"];
}

/**
 * Synchronously scan `PATH` for the `fx50` binary.
 *
 * This is a plain directory lookup (no shell, no `where`/`which` subprocess),
 * which keeps discovery cheap and predictable.
 */
function findOnPath(): string | undefined {
  const pathEnv = process.env.PATH;
  if (!pathEnv) {
    return undefined;
  }
  const directories = pathEnv.split(path.delimiter).filter((dir) => dir.length > 0);
  for (const name of executableNames()) {
    for (const directory of directories) {
      const candidate = path.join(directory, name);
      if (isFile(candidate)) {
        return candidate;
      }
    }
  }
  return undefined;
}

/**
 * Look for a freshly built binary inside the open workspace folders.
 *
 * `cargo build` at the repository root produces `target/debug/fx50`; a release
 * build produces `target/release/fx50`. The release binary is preferred.
 */
function findInWorkspace(): string | undefined {
  const folders = vscode.workspace.workspaceFolders ?? [];
  const relativePaths = ["target/release/fx50", "target/debug/fx50"];
  for (const folder of folders) {
    for (const relative of relativePaths) {
      for (const name of process.platform === "win32" ? [`${relative}.exe`, relative] : [relative]) {
        const candidate = path.join(folder.uri.fsPath, ...name.split("/"));
        if (isFile(candidate)) {
          return candidate;
        }
      }
    }
  }
  return undefined;
}

/**
 * Resolve the `fx50` executable, in order:
 *
 * 1. the non-empty `fx50.serverPath` setting;
 * 2. `fx50` found on `PATH`;
 * 3. `target/release/fx50` or `target/debug/fx50` in a workspace folder.
 *
 * Returns `undefined` when none of these exist.
 */
function resolveServerPath(): string | undefined {
  const configured = vscode.workspace
    .getConfiguration("fx50")
    .get<string>("serverPath", "")
    .trim();
  if (configured.length > 0) {
    return configured;
  }
  return findOnPath() ?? findInWorkspace();
}

/** Map the `fx50.trace.server` setting onto the client's trace level. */
function traceLevel(): Trace {
  const value = vscode.workspace.getConfiguration("fx50").get<string>("trace.server", "off");
  switch (value) {
    case "messages":
      return Trace.Messages;
    case "verbose":
      return Trace.Verbose;
    default:
      return Trace.Off;
  }
}

/** Build (but do not start) the language client for `serverPath`. */
function createClient(serverPath: string): LanguageClient {
  const serverOptions: ServerOptions = {
    command: serverPath,
    args: ["lsp"],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: LANGUAGE_IDS.map((language) => ({ scheme: "file", language })),
    outputChannel,
    traceOutputChannel: outputChannel,
  };

  const created = new LanguageClient(
    "fx50",
    "CASIO fx-50FH II",
    serverOptions,
    clientOptions,
  );
  created.setTrace(traceLevel());
  return created;
}

/** Start the client, reporting a friendly error if the binary cannot run. */
async function startClient(serverPath: string): Promise<void> {
  client = createClient(serverPath);
  try {
    await client.start();
  } catch (error) {
    client = undefined;
    const message = error instanceof Error ? error.message : String(error);
    void vscode.window.showErrorMessage(
      `Failed to start the fx50 language server at ${serverPath}: ${message}`,
    );
  }
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  outputChannel = vscode.window.createOutputChannel("CASIO fx-50FH II");
  context.subscriptions.push(outputChannel);

  const serverPath = resolveServerPath();
  if (!serverPath) {
    const selection = await vscode.window.showErrorMessage(
      "Could not find the `fx50` language server. Build it from this repository with " +
        "`cargo build` (producing target/debug/fx50), or set `fx50.serverPath` to the binary.",
      "Open Settings",
    );
    if (selection === "Open Settings") {
      void vscode.commands.executeCommand("workbench.action.openSettings", "fx50.serverPath");
    }
    return;
  }

  await startClient(serverPath);

  context.subscriptions.push(
    vscode.commands.registerCommand("fx50.restartServer", async () => {
      if (!client) {
        await startClient(serverPath);
        return;
      }
      try {
        await client.restart();
        client.setTrace(traceLevel());
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        void vscode.window.showErrorMessage(`Failed to restart the fx50 language server: ${message}`);
      }
    }),
  );

  // Keep the trace level in sync when the setting changes.
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("fx50.trace.server") && client) {
        client.setTrace(traceLevel());
      }
    }),
  );
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}
