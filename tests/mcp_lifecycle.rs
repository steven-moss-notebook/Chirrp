#![cfg(all(feature = "mcp", not(target_arch = "wasm32")))]

use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::process::{Command, Output};
use std::time::Duration;

struct Instance {
    directory: std::path::PathBuf,
    port: u16,
    token: Option<String>,
}

impl Instance {
    fn new() -> Self {
        Self {
            directory: std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("chirrp-http-test-{}", uuid::Uuid::new_v4())),
            port: 0,
            token: None,
        }
    }

    fn command(&self, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_chirrp-mcp"));
        command.env_remove("CHIRRP_MCP_AUTH_TOKEN");
        if let Some(token) = &self.token {
            command.env("CHIRRP_MCP_AUTH_TOKEN", token);
        }
        command
            .env("CHIRRP_MCP_STATE_DIR", self.directory.join("state"))
            .args(args)
            .output()
            .unwrap()
    }

    fn start(&mut self) {
        let output = self.command(&[
            "start",
            "--output-folder",
            self.directory.join("assets").to_str().unwrap(),
            "--port",
            "0",
        ]);
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            fs::read_to_string(self.directory.join("state/server.log")).unwrap_or_default()
        );
        let state: Value =
            serde_json::from_slice(&fs::read(self.directory.join("state/server.json")).unwrap())
                .unwrap();
        self.port = state["port"].as_u64().unwrap() as u16;
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains(&format!("http://127.0.0.1:{}/mcp", self.port))
        );
    }

    fn http(&self, path: &str, headers: &str, body: &str) -> (u16, String) {
        let (status, body, _) = self.http_method("POST", path, headers, body);
        (status, body)
    }

    fn http_method(
        &self,
        method: &str,
        path: &str,
        headers: &str,
        body: &str,
    ) -> (u16, String, String) {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(stream, "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}", self.port, body.len()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        if headers
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            let mut rest = body;
            let mut decoded = String::new();
            loop {
                let (size, data) = rest.split_once("\r\n").unwrap();
                let size = usize::from_str_radix(size, 16).unwrap();
                if size == 0 {
                    break;
                }
                decoded.push_str(&data[..size]);
                rest = &data[size + 2..];
            }
            (status, decoded, headers.to_owned())
        } else {
            (status, body.to_owned(), headers.to_owned())
        }
    }

    fn request(&self, method: &str, mut params: Value) -> Value {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name":"lifecycle-test","version":"1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        });
        let mut headers = format!("MCP-Protocol-Version: 2026-07-28\r\nMcp-Method: {method}\r\n");
        if let Some(token) = &self.token {
            headers.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        if let Some(name) = params["name"].as_str() {
            headers.push_str(&format!("Mcp-Name: {name}\r\n"));
        }
        let (status, body) = self.http(
            "/mcp",
            &headers,
            &json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string(),
        );
        assert_eq!(status, 200, "{body}");
        let response: Value = serde_json::from_str(&body).unwrap();
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }

    fn tool(&self, name: &str, arguments: Value) -> Value {
        let result = self.request("tools/call", json!({"name":name,"arguments":arguments}));
        assert_eq!(result["isError"], false, "{result}");
        result["structuredContent"].clone()
    }
}

#[test]
fn optional_auth_protects_every_http_method_and_keeps_control_credentials_separate() {
    let mut instance = Instance::new();
    let token = "test-secret-with-at-least-32-characters";
    instance.token = Some(token.into());
    instance.start();
    for method in ["GET", "POST", "DELETE", "OPTIONS"] {
        let (status, body, headers) = instance.http_method(method, "/mcp", "", "{}");
        assert_eq!(status, 401, "{method}: {body}");
        let headers = headers.to_lowercase();
        assert!(headers.contains("www-authenticate: bearer realm=\"chirrp-mcp\""));
        assert!(headers.contains("cache-control: no-store"));
        assert!(!body.contains(token));
    }
    for headers in [
        "Authorization: Bearer wrong\r\n".to_owned(),
        format!("Authorization: Basic {token}\r\n"),
        format!("Authorization: Bearer {token}\r\nAuthorization: Bearer {token}\r\n"),
    ] {
        assert_eq!(instance.http("/mcp", &headers, "{}").0, 401);
    }
    assert_eq!(
        instance
            .http(&format!("/mcp?access_token={token}"), "", "{}")
            .0,
        401
    );
    let asset = instance.tool("generate_sound", json!({"kind":"ui_click"}));
    assert!(std::path::Path::new(asset["path"].as_str().unwrap()).is_file());
    let headers = format!("Authorization: bEaReR {token}\r\nOrigin: https://example.com\r\n");
    assert_eq!(instance.http("/mcp", &headers, "{}").0, 403);
    assert_eq!(
        instance
            .http(
                "/_shutdown",
                &format!("Authorization: Bearer {token}\r\n"),
                ""
            )
            .0,
        403
    );
    let state_text = fs::read_to_string(instance.directory.join("state/server.json")).unwrap();
    assert!(!state_text.contains(token));
    assert!(
        !fs::read_to_string(instance.directory.join("state/server.log"))
            .unwrap()
            .contains(token)
    );
    let state: Value = serde_json::from_str(&state_text).unwrap();
    assert_eq!(
        instance
            .http(
                "/mcp",
                &format!(
                    "Authorization: Bearer {}\r\n",
                    state["token"].as_str().unwrap()
                ),
                "{}"
            )
            .0,
        401
    );
    instance.token = None;
    assert!(
        instance.command(&["stop"]).status.success(),
        "stop does not require the MCP token"
    );
}

#[test]
fn invalid_auth_configuration_fails_closed_without_echoing_secrets() {
    let mut instance = Instance::new();
    for token in [
        "",
        "short",
        "invalid secret with spaces that is long enough",
        &"x".repeat(257),
    ] {
        instance.token = Some(token.into());
        let result = instance.command(&["start", "--port", "0"]);
        assert!(!result.status.success());
        let message = String::from_utf8_lossy(&result.stderr);
        assert!(message.contains("CHIRRP_MCP_AUTH_TOKEN"));
        if !token.is_empty() {
            assert!(!message.contains(token));
        }
        assert!(!instance.directory.join("state/server.json").exists());
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = self.command(&["stop"]);
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn background_http_generate_stop_and_restart() {
    let mut instance = Instance::new();
    instance.start();
    let discovery = instance.request("server/discover", json!({}));
    assert_eq!(
        discovery["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "chirrp"
    );
    assert_eq!(
        instance.request("tools/list", json!({}))["tools"]
            .as_array()
            .unwrap()
            .len(),
        17
    );
    instance.tool("create_sound", json!({"kind":"laser","seed":42}));
    let recipe = instance.tool("get_recipe", json!({"index":0}));
    assert!(
        !recipe.is_null(),
        "editing state must persist between HTTP requests"
    );
    let first = instance.tool("generate_sound", json!({"kind":"laser","name":"shot"}));
    let path = first["path"].as_str().unwrap();
    assert_eq!(&fs::read(path).unwrap()[..4], b"RIFF");
    assert!(std::path::Path::new(path).starts_with(instance.directory.join("assets")));
    // A second start must leave the running process and its editing state intact.
    assert!(!instance.command(&["start", "--port", "0"]).status.success());
    assert_eq!(instance.tool("get_recipe", json!({"index":0})), recipe);
    // No unauthenticated control requests or requests from foreign browser origins.
    assert_eq!(instance.http("/_shutdown", "", "").0, 403);
    assert_eq!(
        instance
            .http("/mcp", "Origin: https://example.com\r\n", "{}")
            .0,
        403
    );
    let stopped = instance.command(&["stop"]);
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    assert!(!instance.directory.join("state/server.json").exists());
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, instance.port)).is_err());
    assert!(instance.command(&["stop"]).status.success());
    instance.start();
    let next = instance.tool("generate_sound", json!({"kind":"laser","name":"shot"}));
    assert_ne!(first["path"], next["path"]);
    assert_eq!(
        fs::read(path).unwrap(),
        fs::read(next["path"].as_str().unwrap()).unwrap()
    );
}

#[test]
fn failed_bind_and_stale_state_are_recoverable() {
    let mut instance = Instance::new();
    let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let output = instance.command(&[
        "start",
        "--port",
        &occupied.local_addr().unwrap().port().to_string(),
    ]);
    assert!(!output.status.success());
    // Stale credentials do not cause stop to contact or kill an unrelated server.
    fs::write(
        instance.directory.join("state/server.json"),
        br#"{"port":1,"token":"stale"}"#,
    )
    .unwrap();
    assert!(instance.command(&["stop"]).status.success());
    instance.start();
    assert!(instance.command(&["stop"]).status.success());
    for args in [
        &["start", "--output-folder"][..],
        &["start", "--port", "65536"],
        &["stop", "--port", "0"],
        &["unknown"],
    ] {
        assert!(!instance.command(args).status.success());
    }
}
