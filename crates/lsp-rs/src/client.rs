use std::collections::HashMap;
use std::io::BufReader;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

use crate::codec::{read_message, write_message};
use crate::{Diagnostic, Location};

pub const INITIALIZE: &str = "initialize";
pub const INITIALIZED: &str = "initialized";
pub const DID_OPEN: &str = "textDocument/didOpen";
pub const DEFINITION: &str = "textDocument/definition";
pub const HOVER: &str = "textDocument/hover";
pub const REFERENCES: &str = "textDocument/references";
pub const PUBLISH_DIAGNOSTICS: &str = "textDocument/publishDiagnostics";
pub const SHUTDOWN: &str = "shutdown";
pub const EXIT: &str = "exit";
const ERR_NO_STDIN: &str = "lsp: server stdin unavailable";
const ERR_NO_STDOUT: &str = "lsp: server stdout unavailable";

pub const ROOT_PATH: &str = "rootPath";

/// Synchronous LSP client speaking JSON-RPC over a server process's stdio.
pub struct LspClient {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    next_id: u64,
    diagnostics: HashMap<String, Vec<Diagnostic>>,
}

impl LspClient {
    /// Spawn a language server and complete the initialize handshake.
    pub fn spawn(cmd: &mut Command, root: &str) -> Result<Self, std::io::Error> {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other(ERR_NO_STDIN))?;
        let reader = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| std::io::Error::other(ERR_NO_STDOUT))?,
        );
        let mut client = Self {
            child,
            stdin,
            reader,
            next_id: 1,
            diagnostics: HashMap::new(),
        };
        client.initialize(root)?;
        Ok(client)
    }

    fn initialize(&mut self, root: &str) -> Result<(), std::io::Error> {
        let params = json!({
            "processId": std::process::id(),
            ROOT_PATH: root,
            "capabilities": {},
        });
        self.request(INITIALIZE, params)?;
        self.notify(INITIALIZED, json!({}))
    }

    /// Send a request and wait for the matching response payload.
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, std::io::Error> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.send(&msg)?;
        let mut body = Vec::new();
        loop {
            read_message(&mut self.reader, &mut body)?;
            let v: Value = serde_json::from_slice(&body)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            if v["id"] == id {
                return Ok(v["result"].clone());
            }
            self.absorb(&v);
        }
    }

    pub fn notify(&mut self, method: &str, params: Value) -> Result<(), std::io::Error> {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.send(&msg)
    }

    fn send(&mut self, msg: &Value) -> Result<(), std::io::Error> {
        write_message(&mut self.stdin, msg.to_string().as_bytes())
    }

    /// File server-pushed notifications (diagnostics) while waiting for responses.
    fn absorb(&mut self, v: &Value) {
        if v["method"].as_str() != Some(PUBLISH_DIAGNOSTICS) {
            return;
        }
        let uri = v["params"]["uri"].as_str().unwrap_or_default().to_string();
        let diags = v["params"]["diagnostics"]
            .as_array()
            .map(|a| a.iter().filter_map(Diagnostic::parse).collect());
        self.diagnostics.insert(uri, diags.unwrap_or_default());
    }

    pub fn diagnostics(&self) -> &HashMap<String, Vec<Diagnostic>> {
        &self.diagnostics
    }

    /// Open a file in the server, converting a UTF-8 column to the LSP UTF-16 position.
    pub fn open(&mut self, uri: &str, language: &str, text: &str) -> Result<(), std::io::Error> {
        self.notify(
            DID_OPEN,
            json!({
                "textDocument": {"uri": uri, "languageId": language, "version": 1, "text": text}
            }),
        )
    }

    pub fn definition(
        &mut self,
        uri: &str,
        pos: [u32; 2],
    ) -> Result<Vec<Location>, std::io::Error> {
        self.locations(DEFINITION, uri, pos, json!({}))
    }

    pub fn hover(&mut self, uri: &str, pos: [u32; 2]) -> Result<Option<String>, std::io::Error> {
        let result = self.request(HOVER, text_document_position(uri, pos))?;
        Ok(result["contents"]
            .as_str()
            .or(result["contents"]["value"].as_str())
            .map(str::to_string))
    }

    pub fn references(
        &mut self,
        uri: &str,
        pos: [u32; 2],
        include_declaration: bool,
    ) -> Result<Vec<Location>, std::io::Error> {
        self.locations(
            REFERENCES,
            uri,
            pos,
            json!({"includeDeclaration": include_declaration}),
        )
    }

    fn locations(
        &mut self,
        method: &str,
        uri: &str,
        pos: [u32; 2],
        context: Value,
    ) -> Result<Vec<Location>, std::io::Error> {
        let result = self.request(
            method,
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": pos[0], "character": pos[1]},
                "context": context,
            }),
        )?;
        let items = match result {
            Value::Null => Vec::new(),
            Value::Array(a) => a,
            other => vec![other],
        };
        Ok(items.iter().filter_map(Location::parse).collect())
    }

    pub fn shutdown(mut self) -> Result<(), std::io::Error> {
        self.request(SHUTDOWN, Value::Null)?;
        self.notify(EXIT, Value::Null)?;
        self.child.wait()?;
        Ok(())
    }
}

fn text_document_position(uri: &str, pos: [u32; 2]) -> Value {
    json!({
        "textDocument": {"uri": uri},
        "position": {"line": pos[0], "character": pos[1]},
    })
}
