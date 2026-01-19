mod completion;
mod diagnostics;
mod parser;
mod workspace;

use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing::info;

use workspace::Workspace;

/// The konf-provider Language Server
pub struct KonfLsp {
    client: Client,
    workspace: Arc<RwLock<Workspace>>,
}

impl KonfLsp {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            workspace: Arc::new(RwLock::new(Workspace::new())),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for KonfLsp {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        info!("Initializing konf-lsp");

        // Index workspace on init
        if let Some(folders) = params.workspace_folders {
            let mut ws = self.workspace.write().await;
            for folder in folders {
                ws.add_folder(&folder.uri);
            }
        } else if let Some(root_uri) = params.root_uri {
            let mut ws = self.workspace.write().await;
            ws.add_folder(&root_uri);
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "konf-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                // Sync full documents
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Enable completion
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        "$".to_string(),
                        "{".to_string(),
                        ".".to_string(),
                        "-".to_string(),
                        " ".to_string(),
                    ]),
                    ..Default::default()
                }),
                // Enable go-to-definition
                definition_provider: Some(OneOf::Left(true)),
                // Enable hover
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Diagnostics are pushed via publish_diagnostics on didOpen/didChange/didSave
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        info!("konf-lsp initialized");
        self.client
            .log_message(MessageType::INFO, "konf-lsp initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        info!("konf-lsp shutting down");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        info!("File opened: {}", uri);

        // Update workspace
        {
            let mut ws = self.workspace.write().await;
            ws.update_document(&uri, &text);
        }

        // Publish diagnostics
        self.publish_diagnostics(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().next() {
            let mut ws = self.workspace.write().await;
            ws.update_document(&uri, &change.text);
        }

        // Publish diagnostics
        self.publish_diagnostics(&uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        info!("File saved: {}", uri);

        // Re-publish diagnostics on save
        self.publish_diagnostics(&uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        info!("File closed: {}", uri);

        // Clear diagnostics
        self.client
            .publish_diagnostics(uri, vec![], None)
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let ws = self.workspace.read().await;
        let items = completion::get_completions(&ws, uri, position);

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let ws = self.workspace.read().await;

        if let Some(location) = completion::goto_definition(&ws, uri, position) {
            Ok(Some(GotoDefinitionResponse::Scalar(location)))
        } else {
            Ok(None)
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let ws = self.workspace.read().await;

        Ok(completion::hover(&ws, uri, position))
    }
}

impl KonfLsp {
    async fn publish_diagnostics(&self, uri: &Url) {
        let ws = self.workspace.read().await;
        let diags = diagnostics::get_diagnostics(&ws, uri);

        self.client
            .publish_diagnostics(uri.clone(), diags, None)
            .await;
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("konf_lsp=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    info!("Starting konf-lsp");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(KonfLsp::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
