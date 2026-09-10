//! The stdio JSON-RPC server implementation.
//!
//! This is a thin shell: all of the real behaviour lives in
//! [`crate::logic`].  The server keeps the text, language and base directory of
//! every open document in memory (full-text sync) so that diagnostics,
//! completion, hover and document symbols can be answered without going back
//! to the client.
//!
//! It is exposed as a library so that the unified `fx50` binary can start it
//! via `fx50 lsp`; `main.rs` is just a shim around [`run_server`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::logic::{self, Language};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbolParams,
    DocumentSymbolResponse, Hover, HoverParams, HoverProviderCapability, InitializeParams,
    InitializeResult, InitializedParams, MessageType, OneOf, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// The state kept for one open document.
#[derive(Clone)]
struct Document {
    /// The full document text.
    text: String,
    /// The language selected when the document was opened.
    language: Language,
    /// The directory used to resolve `#include` paths (the document's parent).
    base_dir: Option<PathBuf>,
}

struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, Document>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Backend {
            client,
            documents: Mutex::new(HashMap::new()),
        }
    }

    /// The parent directory of a `file:` URI, when it has one.
    fn base_dir(uri: &Url) -> Option<PathBuf> {
        uri.to_file_path()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
    }

    /// The language for a document, preferring the client-supplied ID.
    fn language_for(uri: &Url, language_id: Option<&str>) -> Language {
        language_id
            .and_then(Language::from_id)
            .unwrap_or_else(|| Language::from_path(uri.path()))
    }

    fn store(&self, uri: Url, document: Document) {
        self.documents.lock().unwrap().insert(uri, document);
    }

    fn document(&self, uri: &Url) -> Option<Document> {
        self.documents.lock().unwrap().get(uri).cloned()
    }

    async fn publish(&self, uri: &Url, text: &str, language: Language, base_dir: Option<&Path>) {
        let diagnostics = logic::diagnostics(text, language, base_dir);
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "fx-50FH II".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                // Documents are sent to us in full on every change.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    // Pop the list on a function call, a directive or `phys.`.
                    trigger_characters: Some(vec![
                        "(".to_string(),
                        "#".to_string(),
                        ".".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "fx-lsp is ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let language = Self::language_for(&uri, Some(&params.text_document.language_id));
        let base_dir = Self::base_dir(&uri);
        self.store(
            uri.clone(),
            Document {
                text: text.clone(),
                language,
                base_dir: base_dir.clone(),
            },
        );
        self.publish(&uri, &text, language, base_dir.as_deref())
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // Full sync: the last change carries the complete document.
        if let Some(change) = params.content_changes.into_iter().last() {
            // Keep the language chosen at `didOpen`; a change that arrives
            // before the open falls back to the URI's extension.
            let existing = self.document(&uri);
            let language = existing
                .as_ref()
                .map(|document| document.language)
                .unwrap_or_else(|| Language::from_path(uri.path()));
            let base_dir = existing
                .and_then(|document| document.base_dir)
                .or_else(|| Self::base_dir(&uri));
            self.store(
                uri.clone(),
                Document {
                    text: change.text.clone(),
                    language,
                    base_dir: base_dir.clone(),
                },
            );
            self.publish(&uri, &change.text, language, base_dir.as_deref())
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .lock()
            .unwrap()
            .remove(&params.text_document.uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let language = self
            .document(&uri)
            .map(|document| document.language)
            .unwrap_or_else(|| Language::from_path(uri.path()));
        Ok(Some(CompletionResponse::Array(logic::completion_items(
            language,
        ))))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        Ok(self
            .document(&uri)
            .and_then(|document| logic::hover(&document.text, position, document.language)))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        Ok(self.document(uri).map(|document| {
            DocumentSymbolResponse::Nested(logic::document_symbols(
                &document.text,
                document.language,
            ))
        }))
    }
}

/// Run the language server over stdio until the client shuts it down.
///
/// This is a synchronous entry point (it owns a Tokio runtime internally) so
/// that callers such as the `fx50` CLI do not need an async runtime of their
/// own.
pub fn run_server() -> std::io::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let (service, socket) = LspService::new(Backend::new);
        Server::new(stdin, stdout, socket).serve(service).await;
    });
    Ok(())
}
