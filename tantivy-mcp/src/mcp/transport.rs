//! Stdio transport for MCP.

use std::io::{self, BufRead, Write};
use tracing::debug;

use super::types::{JsonRpcRequest, JsonRpcResponse};

/// Stdio transport for MCP communication.
pub struct StdioTransport {
    stdin: io::Stdin,
    stdout: io::Stdout,
}

impl StdioTransport {
    pub fn new() -> Self {
        Self {
            stdin: io::stdin(),
            stdout: io::stdout(),
        }
    }

    /// Read a single JSON-RPC request from stdin.
    pub fn read_request(&mut self) -> Option<JsonRpcRequest> {
        let mut line = String::new();
        match self.stdin.lock().read_line(&mut line) {
            Ok(0) => {
                // EOF
                debug!("Received EOF on stdin");
                None
            }
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    return self.read_request();
                }
                debug!("Received: {}", &line[..line.len().min(200)]);
                match serde_json::from_str::<JsonRpcRequest>(line) {
                    Ok(request) => Some(request),
                    Err(e) => {
                        debug!("Failed to parse request: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                debug!("Error reading from stdin: {}", e);
                None
            }
        }
    }

    /// Write a JSON-RPC response to stdout.
    pub fn write_response(&mut self, response: &JsonRpcResponse) -> io::Result<()> {
        let json = serde_json::to_string(response)?;
        debug!("Sending: {}", &json[..json.len().min(200)]);
        writeln!(self.stdout.lock(), "{}", json)?;
        self.stdout.lock().flush()?;
        Ok(())
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}
