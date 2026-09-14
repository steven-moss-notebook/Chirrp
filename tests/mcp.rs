#![cfg(all(feature = "mcp", not(target_arch = "wasm32")))]

use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<String>,
    directory: PathBuf,
    id: usize,
}
impl Client {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "chirrp-mcp-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut child = Command::new(env!("CARGO_BIN_EXE_chirrp-mcp"))
            // HTTP authentication settings must never gate stdio.
            .env("CHIRRP_MCP_AUTH_TOKEN", "invalid-for-http")
            .arg("stdio")
            .arg("--output-folder")
            .arg(&directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        Self {
            input: child.stdin.take(),
            output,
            child,
            directory,
            id: 0,
        }
    }
    fn send(&mut self, message: &str) {
        writeln!(self.input.as_mut().unwrap(), "{message}").unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
    }
    fn receive(&mut self) -> Value {
        let line = self
            .output
            .recv_timeout(Duration::from_secs(30))
            .expect("server must respond within 30 seconds");
        serde_json::from_str(&line).unwrap()
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        self.send(
            &json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params}).to_string(),
        );
        let response = self.receive();
        assert_eq!(response["id"], self.id);
        assert_eq!(response["jsonrpc"], "2.0");
        response
    }
    fn initialize(&mut self) {
        self.initialize_version("2025-11-25");
    }
    fn initialize_version(&mut self, version: &str) {
        let response = self.request(
            "initialize",
            json!({
                "protocolVersion":version, "capabilities":{},
                "clientInfo":{"name":"integration-test","version":"1"}
            }),
        );
        assert_eq!(response["result"]["protocolVersion"], version);
        assert!(response["result"]["capabilities"]["tools"].is_object());
        self.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    }
    fn tool(&mut self, name: &str, args: Value) -> Value {
        let response = self.request("tools/call", json!({"name":name,"arguments":args}));
        assert_eq!(response["result"]["isError"], false, "{response}");
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    }
    fn tool_error(&mut self, name: &str, args: Value) {
        let response = self.request("tools/call", json!({"name":name,"arguments":args}));
        assert_eq!(response["result"]["isError"], true, "{response}");
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                self.child.kill().unwrap();
                self.child.wait().unwrap();
                panic!("MCP server did not shut down after stdin EOF");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let _ = fs::remove_dir_all(&self.directory);
        if !std::thread::panicking() {
            assert!(status.success());
        }
    }
}

#[test]
fn stdio_lifecycle_discovery_and_protocol_errors() {
    let mut client = Client::new();
    client.initialize();
    let tools = client.request("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 17);
    assert!(
        tools
            .iter()
            .all(|tool| tool["inputSchema"]["type"] == "object")
    );
    let export = tools
        .iter()
        .find(|tool| tool["name"] == "export_wav")
        .unwrap();
    assert_eq!(export["annotations"]["readOnlyHint"], false);
    assert_eq!(export["annotations"]["destructiveHint"], false);
    assert_eq!(export["outputSchema"]["type"], "object");
    let list = tools
        .iter()
        .find(|tool| tool["name"] == "list_sounds")
        .unwrap();
    assert_eq!(list["annotations"]["readOnlyHint"], true);
    assert!(
        tools
            .iter()
            .all(|tool| tool["annotations"]["openWorldHint"] == false)
    );
    assert_eq!(
        client
            .tool("list_sounds", json!({}))
            .as_array()
            .unwrap()
            .len(),
        66
    );
    assert_eq!(client.request("ping", json!({}))["result"], json!({}));
    assert_eq!(
        client.request("unknown", json!({}))["error"]["code"],
        -32601
    );
    assert_eq!(
        client.request("tools/call", json!({"name":"unknown"}))["error"]["code"],
        -32601
    );
    // Notifications never produce responses and must not execute tools.
    client.send(r#"{"jsonrpc":"2.0","method":"notifications/test"}"#);
    client.tool_error("list_candidates", json!({}));
}

#[test]
fn generates_reproducible_game_assets_and_mixes_saved_recipes() {
    let mut client = Client::new();
    client.initialize();
    let first = client.tool(
        "generate_sound",
        json!({"kind":"laser","name":"laser","edits":{"room":0.1},"sample_rate":24000}),
    );
    let path = PathBuf::from(first["path"].as_str().unwrap());
    assert!(path.is_absolute() && path.starts_with(client.directory.canonicalize().unwrap()));
    let wav = fs::read(&path).unwrap();
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 24000);
    assert_eq!(
        wav.len(),
        44 + first["frames"].as_u64().unwrap() as usize * 4
    );
    let saved: Value =
        serde_json::from_slice(&fs::read(first["recipe_path"].as_str().unwrap()).unwrap()).unwrap();
    let repeated = client.tool(
        "mix_sounds",
        json!({"recipes":saved["recipes"],"sample_rate":saved["sample_rate"],"name":"laser"}),
    );
    assert_ne!(first["path"], repeated["path"]);
    assert_eq!(wav, fs::read(repeated["path"].as_str().unwrap()).unwrap());
    assert_eq!(wav, fs::read(path).unwrap());
    let second = client.tool(
        "generate_sound",
        json!({"kind":"impact","sample_rate":24000}),
    );
    let second_saved: Value =
        serde_json::from_slice(&fs::read(second["recipe_path"].as_str().unwrap()).unwrap())
            .unwrap();
    let recipes = json!([saved["recipes"][0], second_saved["recipes"][0]]);
    let mixed = client.tool(
        "mix_sounds",
        json!({"recipes":recipes,"sample_rate":24000,"name":"laser_impact"}),
    );
    let recipes: Vec<chirrp::Recipe> = serde_json::from_value(recipes).unwrap();
    assert_eq!(
        fs::read(mixed["path"].as_str().unwrap()).unwrap(),
        chirrp::render_mix(&recipes, 24000).unwrap().wav_bytes()
    );
    // Stateless exports do not create an editing session.
    client.tool_error("list_candidates", json!({}));
}

#[test]
fn editing_workflow_validates_arguments_and_preserves_state_on_errors() {
    let mut client = Client::new();
    client.initialize();
    client.tool("create_sound", json!({"kind":"footstep","population":2}));
    let before = client.tool("get_recipe", json!({"index":0}));
    client.tool_error("edit_sound", json!({"room":2}));
    client.tool_error("edit_sound", json!({"room":0.5,"typo":1}));
    client.tool_error("create_sound", json!({"kind":"laser","population":1}));
    client.tool_error("select_candidate", json!({"index":0.5}));
    assert_eq!(before, client.tool("get_recipe", json!({"index":0})));
    client.tool("edit_sound", json!({"room":0.2}));
    client.tool("randomize", json!({}));
    client.tool("select_candidate", json!({"index":1}));
    client.tool("evolve", json!({"ratings":[0,1]}));
    assert!(
        client.tool("analyze", json!({"index":0}))["peak"]
            .as_f64()
            .unwrap()
            > 0.
    );
    for name in ["export_wav", "render_audio"] {
        let result = client.tool(name, json!({"index":0}));
        assert!(PathBuf::from(result["path"].as_str().unwrap()).is_file());
    }
    client.tool("random_sound", json!({}));
    assert_eq!(
        client.tool("list_candidates", json!({}))["candidates"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}

#[test]
fn invalid_exports_do_not_write_assets() {
    let mut client = Client::new();
    client.initialize();
    for name in [
        "../escape",
        "/tmp/escape",
        "",
        "a/b",
        "a\\b",
        "é",
        &"a".repeat(81),
    ] {
        client.tool_error("generate_sound", json!({"kind":"ui_click","name":name}));
    }
    client.tool_error("generate_sound", json!({"kind":"unknown"}));
    client.tool_error("generate_sound", json!({"kind":"ui_click","sample_rate":0}));
    client.tool_error("mix_sounds", json!({"recipes":[]}));
    client.tool_error("mix_sounds", json!({"recipes":[{"version":99}]}));
    client.tool_error(
        "mix_sounds",
        json!({"recipes":vec![chirrp::Recipe::new(chirrp::SoundKind::UiClick, 42); 33]}),
    );
    assert_eq!(fs::read_dir(&client.directory).unwrap().count(), 0);
}

#[test]
fn admission_limit_rejects_excess_work_and_recovers_after_cancellation() {
    let mut client = Client::new();
    client.initialize();
    client.id += 1;
    let first = client.id;
    client.send(&json!({"jsonrpc":"2.0","id":first,"method":"tools/call","params":{
        "name":"mix_sounds","arguments":{"recipes":vec![chirrp::Recipe::new(chirrp::SoundKind::Thunder, 42);32],"sample_rate":96000}
    }}).to_string());
    for _ in 0..32 {
        client.id += 1;
        client.send(
            &json!({"jsonrpc":"2.0","id":client.id,"method":"tools/call","params":{
                "name":"list_sounds","arguments":{}
            }})
            .to_string(),
        );
    }
    let busy = client.receive();
    assert_eq!(busy["result"]["isError"], true, "{busy}");
    assert!(
        busy["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("server busy")
    );
    for id in first..=client.id {
        client.send(
            &json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":id}})
                .to_string(),
        );
    }
    assert_eq!(client.request("ping", json!({}))["result"], json!({}));
    client.tool("list_sounds", json!({}));
    assert_eq!(fs::read_dir(&client.directory).unwrap().count(), 0);
}

#[test]
fn sdk_supports_current_protocol_and_structured_results() {
    let mut client = Client::new();
    // The current protocol uses SDK discovery and per-request metadata instead
    // of the legacy initialize/initialized handshake exercised above.
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientInfo":{"name":"integration-test","version":"1"},
        "io.modelcontextprotocol/clientCapabilities":{}
    });
    let discovery = client.request("server/discover", json!({"_meta":meta}));
    assert!(discovery.get("error").is_none(), "{discovery}");
    let response = client.request(
        "tools/call",
        json!({"_meta":meta,"name":"generate_sound","arguments":{"kind":"ui_hover"}}),
    );
    let result = &response["result"];
    assert_eq!(result["isError"], false, "{response}");
    assert_eq!(result["resultType"], "complete");
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, result["structuredContent"]);
    assert!(text["path"].is_string());
}

#[test]
fn rendering_keeps_protocol_responsive_and_cancellation_prevents_export() {
    let mut client = Client::new();
    client.initialize();
    let recipe = chirrp::Recipe::new(chirrp::SoundKind::Thunder, 42);
    client.id += 1;
    let render_id = client.id;
    client.send(
        &json!({"jsonrpc":"2.0","id":render_id,"method":"tools/call","params":{
            "name":"mix_sounds","arguments":{"recipes":vec![recipe; 20],"sample_rate":96000}
        }})
        .to_string(),
    );
    client.id += 1;
    let queued_id = client.id;
    client.send(
        &json!({"jsonrpc":"2.0","id":queued_id,"method":"tools/call","params":{
            "name":"generate_sound","arguments":{"kind":"ui_click"}
        }})
        .to_string(),
    );
    // A ping must complete while the sound worker is still busy.
    assert_eq!(client.request("ping", json!({}))["result"], json!({}));
    for id in [queued_id, render_id] {
        client.send(
            &json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{
                "requestId":id,"reason":"test cancellation"
            }})
            .to_string(),
        );
    }
    // The SDK suppresses responses to canceled requests.
    // This waits for any still-running DSP to release the worker. Cancellation
    // is checked between recipes and again before writing files.
    client.tool("list_sounds", json!({}));
    assert_eq!(fs::read_dir(&client.directory).unwrap().count(), 0);
}

#[test]
fn invalid_or_oversized_frames_close_the_sdk_transport() {
    for message in ["not JSON".to_string(), "x".repeat(1024 * 1024 + 1)] {
        let mut client = Client::new();
        client.initialize();
        client.send(&message);
        assert!(matches!(
            client.output.recv_timeout(Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
}

#[test]
fn additive_asset_tools_export_loops_mono_and_reproducible_timed_layers() {
    let mut client = Client::new();
    client.initialize();
    client.tool("create_sound", json!({"kind":"laser"}));
    let before = client.tool("get_recipe", json!({"index":0}));
    let result = client.tool("generate_loop", json!({
        "kind":"hull_rumble","loop_s":0.5,"sample_rate":24000,"mono":true,"dry_mid":true,"name":"hull"
    }));
    assert_eq!(result["channels"], 1);
    assert_eq!(result["frames"], 12000);
    let wav = fs::read(result["path"].as_str().unwrap()).unwrap();
    assert_eq!(&wav[44 + 24000..44 + 24004], b"smpl");
    let sidecar: Value =
        serde_json::from_slice(&fs::read(result["recipe_path"].as_str().unwrap()).unwrap())
            .unwrap();
    let restored = client.tool("render_sound", sidecar["render_sound"].clone());
    assert_eq!(wav, fs::read(restored["path"].as_str().unwrap()).unwrap());
    let shot = client.tool(
        "render_sound",
        json!({"recipe":before,"dry_mid":true,"mono":true}),
    );
    let sidecar: Value =
        serde_json::from_slice(&fs::read(shot["recipe_path"].as_str().unwrap()).unwrap()).unwrap();
    assert!(sidecar["render_sound"].get("loop_s").is_none());
    let replay = client.tool("render_sound", sidecar["render_sound"].clone());
    assert_eq!(
        fs::read(shot["path"].as_str().unwrap()).unwrap(),
        fs::read(replay["path"].as_str().unwrap()).unwrap()
    );
    let layers = client.tool("mix_layers", json!({
        "layers":[{"recipe":before,"gain":0.5,"delay_s":0.2}],"sample_rate":24000,"mono":true,"dry_mid":true
    }));
    let sidecar: Value =
        serde_json::from_slice(&fs::read(layers["recipe_path"].as_str().unwrap()).unwrap())
            .unwrap();
    assert!(
        (sidecar["mix_layers"]["layers"][0]["delay_s"]
            .as_f64()
            .unwrap()
            - 0.2)
            .abs()
            < 1e-6
    );
    let restored = client.tool("mix_layers", sidecar["mix_layers"].clone());
    assert_eq!(
        fs::read(layers["path"].as_str().unwrap()).unwrap(),
        fs::read(restored["path"].as_str().unwrap()).unwrap()
    );
    for (name, args) in [
        ("generate_loop", json!({"kind":"hull_rumble","loop_s":17})),
        (
            "generate_loop",
            json!({"kind":"hull_rumble","loop_s":1,"mono":"yes"}),
        ),
        (
            "mix_layers",
            json!({"layers":[{"recipe":before,"delay_s":-1}]}),
        ),
        ("mix_layers", json!({"layers":[]})),
    ] {
        client.tool_error(name, args);
    }
    assert_eq!(before, client.tool("get_recipe", json!({"index":0})));
}
