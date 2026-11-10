use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
fn main() {
    let root = studio_service::workspace_root();
    let write_enabled = std::env::args().any(|a| a == "--allow-write");
    let mut initialized = false;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut out = io::stdout().lock();
    loop {
        let mut bytes = vec![];
        let mut oversized = false;
        loop {
            let buf = match input.fill_buf() {
                Ok(b) => b,
                Err(_) => return,
            };
            if buf.is_empty() {
                if bytes.is_empty() {
                    return;
                }
                break;
            }
            let count = buf
                .iter()
                .position(|b| *b == b'\n')
                .map(|n| n + 1)
                .unwrap_or(buf.len());
            let done = buf[count - 1] == b'\n';
            if bytes.len() + count <= 1_048_576 {
                bytes.extend_from_slice(&buf[..count]);
            } else {
                oversized = true;
            }
            input.consume(count);
            if done {
                break;
            }
        }
        let response = if oversized {
            Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Message exceeds 1 MiB"}}),
            )
        } else {
            match serde_json::from_slice::<Value>(&bytes) {
                Ok(message) => {
                    studio_service::mcp_message(message, &mut initialized, write_enabled, |r| {
                        studio_service::call(&root, &r)
                    })
                }
                Err(_) => Some(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                ),
            }
        };
        if let Some(response) = response {
            if writeln!(out, "{response}")
                .and_then(|_| out.flush())
                .is_err()
            {
                return;
            }
        }
    }
}
