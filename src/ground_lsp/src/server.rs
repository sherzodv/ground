use std::io::{self, BufReader, BufWriter};

use serde_json::{json, Value};

use crate::features::{
    completion::completion,
    definition::definition,
    formatting::formatting,
    hover::hover,
    semantic_tokens::semantic_tokens,
};
use crate::protocol::{read_message, send_notification, send_response};
use crate::workspace::Workspace;

struct Server {
    workspace: Workspace,
    shutdown_requested: bool,
}

impl Server {
    fn new(root: std::path::PathBuf) -> Self {
        Self { workspace: Workspace::new(root), shutdown_requested: false }
    }

    fn handle(&mut self, msg: Value, out: &mut dyn io::Write) -> io::Result<bool> {
        let method = msg.get("method").and_then(Value::as_str);
        let id = msg.get("id").cloned();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        match method {
            Some("initialize") => {
                let result = json!({
                    "capabilities": {
                        "textDocumentSync": 1,
                        "completionProvider": {
                            "resolveProvider": false,
                            "triggerCharacters": [" ", ":"]
                        },
                        "hoverProvider": true,
                        "definitionProvider": true,
                        "semanticTokensProvider": {
                            "legend": {
                                "tokenTypes": ["keyword", "namespace", "type", "function", "property", "enumMember", "variable", "string", "number", "comment", "class"],
                                "tokenModifiers": []
                            },
                            "full": true
                        },
                        "documentFormattingProvider": true
                    },
                    "serverInfo": { "name": "ground" }
                });
                send_response(out, id, result)?;
            }
            Some("initialized") => self.publish_all(out)?,
            Some("shutdown") => {
                self.shutdown_requested = true;
                send_response(out, id, Value::Null)?;
            }
            Some("exit") => return Ok(self.shutdown_requested),
            Some("textDocument/didOpen") => {
                self.workspace.did_open(&params);
                let _ = self.workspace.reload();
                self.publish_all(out)?;
            }
            Some("textDocument/didChange") => {
                self.workspace.did_change(&params);
                let _ = self.workspace.reload();
                self.publish_all(out)?;
            }
            Some("textDocument/didClose") => {
                self.workspace.did_close(&params);
                let _ = self.workspace.reload();
                self.publish_all(out)?;
            }
            Some("textDocument/completion") => send_response(out, id, Value::Array(completion(&self.workspace, &params)))?,
            Some("textDocument/definition") => send_response(out, id, definition(&self.workspace, &params).unwrap_or(Value::Null))?,
            Some("textDocument/hover") => send_response(out, id, hover(&self.workspace, &params).unwrap_or(Value::Null))?,
            Some("textDocument/semanticTokens/full") => send_response(out, id, semantic_tokens(&self.workspace, &params))?,
            Some("textDocument/formatting") => send_response(out, id, Value::Array(formatting(&self.workspace, &params)))?,
            _ => {
                if id.is_some() {
                    send_response(out, id, Value::Null)?;
                }
            }
        }
        Ok(false)
    }

    fn publish_all(&self, out: &mut dyn io::Write) -> io::Result<()> {
        for (uri, diagnostics) in self.workspace.diagnostics_by_uri() {
            send_notification(out, "textDocument/publishDiagnostics", json!({
                "uri": uri,
                "diagnostics": diagnostics,
            }))?;
        }
        Ok(())
    }
}

pub fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    let mut server = Server::new(std::env::current_dir()?);
    let _ = server.workspace.reload();
    while let Some(msg) = read_message(&mut reader)? {
        if server.handle(msg, &mut writer)? {
            break;
        }
    }
    Ok(())
}
