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
fn http_enforces_authentication_session_ownership_and_rate_limits() {
    let directory = std::env::temp_dir().join(format!("logic-mcp-http-{}", std::process::id()));
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
    let other = "b".repeat(64);
    let other_digest = format!("{:x}", Sha256::digest(other.as_bytes()));
    let config = directory.0.join("clients.json");
    fs::write(
        &config,
        serde_json::json!({
            "requests_per_minute": 6,
            "clients":[
                {"id":"test","token_sha256":digest,"scopes":["read"]},
                {"id":"other","token_sha256":other_digest,"scopes":["read"]}
            ]
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
    let request = |path: &str, host: &str, extra: &str, payload: &str| {
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(client, "POST {path} HTTP/1.1\r\nHost: {host}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{payload}", payload.len()).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        response
    };
    let status = |path: &str, host: &str, extra: &str| {
        let response = request(path, host, extra, &body);
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
    assert_eq!(status("/mcp/portable", &address.to_string(), ""), 401);
    assert_eq!(
        status(
            "/mcp",
            &address.to_string(),
            &format!("Authorization: Bearer {}\r\n", "c".repeat(64))
        ),
        401
    );
    let bearer = format!("Authorization: Bearer {token}\r\n");
    let response = request("/mcp", &address.to_string(), &bearer, &body);
    assert!(response.starts_with("HTTP/1.1 200"));
    let session = response
        .lines()
        .find_map(|line| line.strip_prefix("mcp-session-id: "))
        .expect("legacy session");
    let cross_client = format!("Authorization: Bearer {other}\r\nMcp-Session-Id: {session}\r\n");
    assert_eq!(status("/mcp", &address.to_string(), &cross_client), 404);
    for _ in 0..5 {
        assert_eq!(status("/mcp", &address.to_string(), &bearer), 200);
    }
    let response = request("/mcp", &address.to_string(), &bearer, &body);
    assert!(response.starts_with("HTTP/1.1 429"));
    assert!(response.lines().any(|line| line == "retry-after: 60"));
    let portable = "/mcp/portable";
    let address = address.to_string();
    let other_bearer = format!("Authorization: Bearer {other}\r\n");
    let response = request(portable, &address, &other_bearer, &body);
    assert!(response.starts_with("HTTP/1.1 200"));
    let session = response
        .lines()
        .find_map(|line| line.strip_prefix("mcp-session-id: "))
        .expect("portable session");
    let headers = format!("{other_bearer}Mcp-Session-Id: {session}\r\n");
    request(
        portable,
        &address,
        &headers,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    let tools = request(
        portable,
        &address,
        &headers,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    );
    assert!(tools.contains(r#""name":"sample_list_objects""#));
    assert!(!tools.contains(r#""name":"sample.list_objects""#));
    let result = request(
        portable,
        &address,
        &headers,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"sample_list_objects","arguments":{}}}"#,
    );
    assert!(
        result.contains(r#""structuredContent":{"result""#),
        "{result}"
    );
}
