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
}

impl Instance {
    fn new() -> Self {
        Self {
            directory: std::env::temp_dir()
                .join(format!("chirrp-http-test-{}", uuid::Uuid::new_v4())),
            port: 0,
        }
    }

    fn command(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_chirrp-mcp"))
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
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(stream, "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}", self.port, body.len()).unwrap();
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
            (status, decoded)
        } else {
            (status, body.to_owned())
        }
    }

    fn request(&self, method: &str, mut params: Value) -> Value {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name":"lifecycle-test","version":"1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        });
        let (status, body) = self.http(
            "/mcp",
            "MCP-Protocol-Version: 2026-07-28\r\n",
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
    assert_eq!(discovery["serverInfo"]["name"], "chirrp");
    assert_eq!(
        instance.request("tools/list", json!({}))["tools"]
            .as_array()
            .unwrap()
            .len(),
        14
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
