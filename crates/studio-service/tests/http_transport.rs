use sha2::{Digest, Sha256};
use std::{
    fs::{self, DirBuilder},
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "Requires binding a loopback TCP socket and spawning the MCP binary"]
fn http_rejects_foreign_host_and_origin() {
    let directory = std::env::temp_dir().join(format!("logic-mcp-http-{}", uuid::Uuid::new_v4()));
    DirBuilder::new().mode(0o700).create(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let directory = Cleanup(directory);
    let token = "a".repeat(64);
    let digest = format!("{:x}", Sha256::digest(token.as_bytes()));
    let config = directory.0.join("clients.json");
    fs::write(
        &config,
        serde_json::json!({
            "clients":[{"id":"test","token_sha256":digest,"scopes":["read"]}]
        })
        .to_string(),
    )
    .unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_logic-mcp"))
            .args(["--sample", "--http", "0", "--http-security"])
            .arg(&directory.0)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut ready = String::new();
    BufReader::new(server.0.stderr.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    let address: SocketAddr = ready
        .trim()
        .strip_prefix("MCP Streamable HTTP listening on http://")
        .and_then(|url| url.strip_suffix("/mcp"))
        .unwrap_or_else(|| panic!("Unexpected startup message: {ready}"))
        .parse()
        .unwrap();
    assert!(address.ip().is_loopback());

    let body = serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"protocolVersion":"2025-11-25","capabilities":{},
            "clientInfo":{"name":"http-test","version":"1"}}
    })
    .to_string();
    let status = |path: &str, host: &str, extra: &str| {
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(client, "POST {path} HTTP/1.1\r\nHost: {host}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}", body.len()).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        response
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u16>()
            .unwrap()
    };
    assert_eq!(status("/mcp", "external.example", ""), 403);
    assert_eq!(
        status(
            "/mcp",
            &address.to_string(),
            "Origin: https://external.example\r\n"
        ),
        403
    );
    assert_eq!(status("/elsewhere", &address.to_string(), ""), 404);
    // Native clients such as Inspector do not need to send Origin.
    assert_eq!(status("/mcp", &address.to_string(), ""), 401);
    assert_eq!(
        status(
            "/mcp",
            &address.to_string(),
            &format!("Authorization: Bearer {token}\r\n")
        ),
        200
    );
}
