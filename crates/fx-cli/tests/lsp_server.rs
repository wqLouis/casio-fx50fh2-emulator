//! End-to-end test for `fx50 lsp`: boot the real unified binary in language
//! server mode and speak a little JSON-RPC over its stdio pipes.
//!
//! This lives in the CLI crate because `fx50` is the only binary the project
//! ships; the framing helpers are deliberately minimal (no serde) so the test
//! depends on the standard library only.
#![cfg(feature = "lsp")]

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn send(stdin: &mut ChildStdin, message: &str) {
    write!(
        stdin,
        "Content-Length: {}\r\n\r\n{}",
        message.len(),
        message
    )
    .unwrap();
    stdin.flush().unwrap();
}

/// Read one framed JSON-RPC message.
fn recv(stdout: &mut BufReader<ChildStdout>) -> String {
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        let read = stdout.read_line(&mut line).unwrap();
        assert!(read > 0, "server closed stdout unexpectedly");
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = value.trim().parse().unwrap();
        }
    }
    let mut body = vec![0u8; content_length];
    stdout.read_exact(&mut body).unwrap();
    String::from_utf8(body).unwrap()
}

/// Read messages until one contains `needle`, up to `limit` messages.
fn recv_until(stdout: &mut BufReader<ChildStdout>, needle: &str, limit: usize) -> String {
    for _ in 0..limit {
        let message = recv(stdout);
        if message.contains(needle) {
            return message;
        }
    }
    panic!("did not receive a message containing {needle:?}");
}

fn spawn() -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fx50"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn `fx50 lsp`");
    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    (child, stdin, stdout)
}

#[test]
fn server_initializes_reports_diagnostics_and_shuts_down() {
    let (mut child, mut stdin, mut stdout) = spawn();

    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":null,"capabilities":{}}}"#,
    );
    let init = recv_until(&mut stdout, "\"id\":1", 5);
    for capability in [
        "documentSymbolProvider",
        "completionProvider",
        "hoverProvider",
    ] {
        assert!(
            init.contains(capability),
            "initialize response missing {capability}: {init}"
        );
    }

    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    );
    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///test.fx","languageId":"fx","version":1,"text":"A@B"}}}"#,
    );
    let published = recv_until(&mut stdout, "publishDiagnostics", 5);
    assert!(
        published.contains("Syntax ERROR"),
        "diagnostic missing Syntax ERROR: {published}"
    );

    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
    );
    let _ = recv_until(&mut stdout, "\"id\":2", 5);

    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
    );
    drop(stdin);
    let status = child.wait().expect("wait for fx50 lsp");
    assert!(status.success(), "server exited with {status}");
}

/// A mode violation must surface over the wire as a `Mode ERROR` diagnostic.
#[test]
fn mode_errors_are_published() {
    let (mut child, mut stdin, mut stdout) = spawn();

    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":null,"capabilities":{}}}"#,
    );
    let _ = recv_until(&mut stdout, "\"id\":1", 5);
    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    );

    // `3+4i` needs CMPLX; without a header this is a mode error.
    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///cmplx.fx","languageId":"fx","version":1,"text":"3+4i"}}}"#,
    );
    let published = recv_until(&mut stdout, "publishDiagnostics", 5);
    assert!(
        published.contains("Mode ERROR"),
        "diagnostic missing Mode ERROR: {published}"
    );

    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
    );
    let _ = recv_until(&mut stdout, "\"id\":2", 5);
    send(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
    );
    drop(stdin);
    let _ = child.wait();
}
