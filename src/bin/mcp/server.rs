//! Shared MCP tools and bounded stdio transport.
use chirrp::{AudioBuffer, Engine, Recipe, SoundEdits, SoundKind, mix, render};
use futures_util::StreamExt;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorCode,
    Implementation, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
    ToolAnnotations,
};
use rmcp::service::{RequestContext, RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::async_rw::JsonRpcMessageCodec;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::sync::CancellationToken;

pub(super) type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub(super) const MAX_REQUEST: usize = 1024 * 1024;
const MAX_TOOL_CALLS: usize = 32;
const TOOL_TIMEOUT: Duration = Duration::from_secs(120);

pub async fn serve_stdio(output: PathBuf) -> Result<()> {
    let server = Server::new(output)?;
    eprintln!("Chirrp MCP ready on stdio");
    // Use the SDK's framing with a bounded reader. Malformed or oversized
    // frames close this connection; diagnostics never enter protocol stdout.
    let (stdin, stdout) = rmcp::transport::stdio();
    let reader = FramedRead::new(
        stdin,
        JsonRpcMessageCodec::<RxJsonRpcMessage<RoleServer>>::new_with_max_length(MAX_REQUEST),
    )
    .take_while(|message| {
        if let Err(error) = message {
            eprintln!("chirrp-mcp transport: {error}");
        }
        std::future::ready(message.is_ok())
    })
    .map(|message| message.expect("errors terminate the input stream"));
    let writer = FramedWrite::new(
        stdout,
        JsonRpcMessageCodec::<TxJsonRpcMessage<RoleServer>>::new(),
    );
    let service = server.clone().serve((writer, reader)).await?;
    let cancellation = service.cancellation_token();
    let shutdown = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancellation.cancel();
        }
    });
    let result = service.waiting().await;
    shutdown.abort();
    server.finish_work().await;
    result?;
    Ok(())
}

fn definitions() -> Result<Vec<Tool>> {
    let mut tools: Vec<_> = chirrp::tool_definitions().into_iter().map(|tool| {
        json!({"name":tool.name,"description":tool.description,"inputSchema":tool.input_schema})
    }).collect();
    let name = json!({"type":"string","minLength":1,"maxLength":80,"pattern":"^[A-Za-z0-9_-]+$","description":"Optional asset directory label, using ASCII letters, digits, hyphens or underscores (1–80 characters). A numeric suffix prevents overwrites."});
    for tool in &mut tools {
        if matches!(
            tool["name"].as_str(),
            Some("render_audio" | "export_wav" | "render_sound" | "generate_loop" | "mix_layers")
        ) {
            if matches!(tool["name"].as_str(), Some("render_audio" | "export_wav")) {
                tool["description"] = json!(
                    "Render a candidate to a local PCM16 stereo WAV and recipe sidecar. Returns absolute paths and audio metrics; no PCM arrays. Every call creates a new asset directory."
                );
            }
            tool["inputSchema"]["properties"]["name"] = name.clone();
        }
    }
    let create = tools.iter().find(|t| t["name"] == "create_sound").unwrap();
    let kind = create["inputSchema"]["properties"]["kind"].clone();
    let seed = create["inputSchema"]["properties"]["seed"].clone();
    let edits = tools.iter().find(|t| t["name"] == "edit_sound").unwrap()["inputSchema"].clone();
    let rate = json!({"type":"integer","minimum":22050,"maximum":96000,"default":48000});
    tools.push(json!({
        "name":"generate_sound",
        "description":"Generate a game sound in one call and save WAV plus reproducible recipes.json. Choose a kind using list_sounds. Optional edits tune the preset. Returns absolute paths and metrics. Does not replace the active editing session.",
        "inputSchema":{"type":"object","additionalProperties":false,"required":["kind"],"properties":{
            "kind":kind,"seed":seed,"edits":edits,"sample_rate":rate,"name":name
        }}
    }));
    tools.push(json!({
        "name":"mix_sounds",
        "description":"Render and layer 1–32 recipe objects (from get_recipe or the recipes array in a saved sidecar). Saves a stereo WAV and all recipes. Sounds start together, preserve the longest tail, and share a 0.89 peak ceiling. Accepts one recipe to re-render a saved sound. Does not change the active session.",
        "inputSchema":{"type":"object","additionalProperties":false,"required":["recipes"],"properties":{
            "recipes":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"object","description":"A complete Chirrp recipe object, including version, kind, seed, and genome."}},
            "sample_rate":rate,"name":name
        }}
    }));
    for tool in &mut tools {
        if matches!(
            tool["name"].as_str(),
            Some(
                "generate_sound"
                    | "mix_sounds"
                    | "render_audio"
                    | "export_wav"
                    | "render_sound"
                    | "generate_loop"
                    | "mix_layers"
            )
        ) {
            tool["outputSchema"] = json!({"type":"object","additionalProperties":false,
                "required":["path","recipe_path","format","sample_rate","channels","frames","metrics"],
                "properties":{
                    "path":{"type":"string"},"recipe_path":{"type":"string"},"format":{"const":"wav"},
                    "sample_rate":{"type":"integer","minimum":22050,"maximum":96000},
                    "channels":{"const":2},"frames":{"type":"integer","minimum":1},
                    "metrics":{"type":"object","required":["peak","rms","stereo_correlation","duration_seconds"],"properties":{
                        "peak":{"type":"number"},"rms":{"type":"number"},
                        "stereo_correlation":{"type":"number"},"duration_seconds":{"type":"number"}
                    },"additionalProperties":false}
                }
            });
            if matches!(
                tool["name"].as_str(),
                Some("render_sound" | "generate_loop" | "mix_layers")
            ) {
                tool["outputSchema"]["properties"]["channels"] =
                    json!({"type":"integer","enum":[1,2]});
            }
        } else if tool["name"] == "list_sounds" {
            tool["outputSchema"] = json!({"type":"object","required":["sounds"],"properties":{
                "sounds":{"type":"array","items":{"type":"object","required":["kind","label","description"],"properties":{
                    "kind":{"type":"string"},"label":{"type":"string"},"description":{"type":"string"}
                },"additionalProperties":false}}
            },"additionalProperties":false});
        }
    }
    tools
        .into_iter()
        .map(|value| {
            let tool: Tool = serde_json::from_value(value)?;
            let read_only = matches!(
                tool.name.as_ref(),
                "list_sounds" | "list_candidates" | "get_recipe" | "analyze"
            );
            let replaces_state = matches!(
                tool.name.as_ref(),
                "create_sound" | "random_sound" | "edit_sound" | "randomize" | "evolve"
            );
            let idempotent = read_only || tool.name == "select_candidate";
            Ok(tool.with_annotations(
                ToolAnnotations::new()
                    .read_only(read_only)
                    .destructive(replaces_state)
                    .idempotent(idempotent)
                    .open_world(false),
            ))
        })
        .collect()
}

#[derive(Clone)]
pub(super) struct Server {
    worker: Arc<Mutex<Worker>>,
    tools: Vec<Tool>,
    capacity: Arc<Semaphore>,
    tool_timeout: Duration,
}

impl Server {
    pub(super) fn new(output: PathBuf) -> Result<Self> {
        fs::create_dir_all(&output)?;
        Ok(Self {
            worker: Arc::new(Mutex::new(Worker {
                engine: Engine::default(),
                output: output.canonicalize()?,
            })),
            tools: definitions()?,
            capacity: Arc::new(Semaphore::new(MAX_TOOL_CALLS)),
            tool_timeout: TOOL_TIMEOUT,
        })
    }

    pub(super) async fn finish_work(&self) {
        // The blocking renderer retains this guard until synthesis/export ends.
        drop(self.worker.lock().await);
    }
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("chirrp", env!("CARGO_PKG_VERSION")))
            .with_instructions("Generate local game sound assets with generate_sound. Use list_sounds to discover presets. For iterative design: create_sound, edit_sound/randomize/evolve, then export_wav. Each process has one shared editing session, including authenticated HTTP clients. Get recipes before replacing it to mix different categories with mix_sounds (up to 32 layers). Exports return local WAV paths and recipes.json sidecars; the host handles playback. Tool operations run offline and are serialized, with at most 32 outstanding calls and a 120-second deadline including queue time. Busy errors may be retried later; after a timeout or cancellation, check for an export before retrying a write.")
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|tool| tool.name == name).cloned()
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        if request.and_then(|params| params.cursor).is_some() {
            return Err(ErrorData::invalid_params(
                "tool list is not paginated; omit cursor",
                None,
            ));
        }
        Ok(ListToolsResult {
            tools: self.tools.clone(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        let tool = self
            .get_tool(&request.name)
            .ok_or_else(|| ErrorData::new(ErrorCode::METHOD_NOT_FOUND, "unknown tool", None))?;
        let mut args = Value::Object(request.arguments.unwrap_or_default());
        if let Err(error) = validate(&tool.schema_as_json_value(), &mut args, &tool.name) {
            return Ok(tool_error(error.to_string()).into());
        }
        let Ok(permit) = self.capacity.clone().try_acquire_owned() else {
            return Ok(tool_error(
                "server busy: at most 32 tool calls may be outstanding; retry later".into(),
            )
            .into());
        };
        let cancellation = context.ct.child_token();
        // A timeout, cancellation, or dropped request must also cancel its worker.
        let _cancel_on_drop = cancellation.clone().drop_guard();
        let deadline = tokio::time::Instant::now() + self.tool_timeout;
        // Wait asynchronously for exclusive access to the session. Moving the
        // owned guard into spawn_blocking prevents canceled tasks from releasing
        // it while a render is still running, bounding CPU and memory use.
        let mut worker = tokio::select! {
            biased;
            _ = context.ct.cancelled() => return Ok(tool_error("tool call cancelled".into()).into()),
            _ = tokio::time::sleep_until(deadline) => return Ok(tool_error("tool call timed out while queued; retry later".into()).into()),
            worker = self.worker.clone().lock_owned() => worker,
        };
        let work = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            check_cancelled(&cancellation)?;
            worker.call(&tool.name, args, &cancellation)
        });
        let result = tokio::select! {
            biased;
            _ = context.ct.cancelled() => return Ok(tool_error("tool call cancelled".into()).into()),
            _ = tokio::time::sleep_until(deadline) => return Ok(tool_error("tool call timed out; check for an export before retrying".into()).into()),
            result = work => result,
        }.map_err(|error| {
            eprintln!("chirrp-mcp worker: {error}");
            ErrorData::internal_error("sound worker failed", None)
        })?;
        Ok(match result {
            Ok(value) if value.is_object() => CallToolResult::structured(value),
            Ok(value) => {
                // Preserve the existing list_sounds text payload while adding
                // the object-shaped structured result required by MCP.
                let mut result =
                    CallToolResult::success(vec![ContentBlock::text(value.to_string())]);
                result.structured_content = Some(json!({"sounds":value}));
                result
            }
            Err(error) => tool_error(error.to_string()),
        }
        .into())
    }
}

fn tool_error(message: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<()> {
    if cancellation.is_cancelled() {
        return Err("tool call cancelled".into());
    }
    Ok(())
}

struct Worker {
    engine: Engine,
    output: PathBuf,
}

impl Worker {
    fn call(&mut self, name: &str, args: Value, cancellation: &CancellationToken) -> Result<Value> {
        let value = match name {
            "list_sounds" => json!(self.engine.list_sounds()),
            "create_sound" => json!(self.engine.create_sound(
                arg(&args, "kind")?,
                arg(&args, "seed")?,
                arg(&args, "population")?
            )?),
            "random_sound" => json!(
                self.engine
                    .random_sound(arg(&args, "seed")?, arg(&args, "population")?)?
            ),
            "list_candidates" => json!(self.engine.list_candidates()?),
            "select_candidate" => json!(self.engine.select_candidate(arg(&args, "index")?)?),
            "randomize" => json!(
                self.engine
                    .randomize(arg(&args, "strength")?, arg(&args, "seed")?)?
            ),
            "evolve" => json!(self.engine.evolve(
                &arg::<Vec<f32>>(&args, "ratings")?,
                arg(&args, "strength")?,
                arg(&args, "seed")?
            )?),
            "edit_sound" => json!(self.engine.edit_sound(serde_json::from_value(args)?)?),
            "get_recipe" => json!(self.engine.get_recipe(arg(&args, "index")?)?),
            "analyze" => json!(
                self.engine
                    .analyze(arg(&args, "index")?, arg(&args, "sample_rate")?)?
            ),
            "render_audio" | "export_wav" => {
                let recipe = self.engine.get_recipe(arg(&args, "index")?)?;
                let audio = render(&recipe, arg(&args, "sample_rate")?)?;
                self.save(&audio, &[recipe], &args, cancellation)?
            }
            "generate_sound" => {
                let kind: SoundKind = arg(&args, "kind")?;
                let mut engine = Engine::default();
                engine.create_sound(kind, arg(&args, "seed")?, 2)?;
                if let Some(edits) = args.get("edits") {
                    engine.edit_sound(serde_json::from_value::<SoundEdits>(edits.clone())?)?;
                }
                let recipe = engine.get_recipe(0)?;
                let audio = render(&recipe, arg(&args, "sample_rate")?)?;
                self.save(&audio, &[recipe], &args, cancellation)?
            }
            "render_sound" | "generate_loop" | "mix_layers" => {
                let mut request = args.clone();
                request.as_object_mut().unwrap().remove("name");
                let asset = chirrp::execute_asset_tool(name, request)?;
                self.save_asset(&asset, &args, cancellation)?
            }
            "mix_sounds" => {
                let recipes: Vec<Recipe> = arg(&args, "recipes")?;
                for recipe in &recipes {
                    recipe.validate()?;
                }
                let sample_rate = arg(&args, "sample_rate")?;
                let mut sounds = Vec::with_capacity(recipes.len());
                for recipe in &recipes {
                    check_cancelled(cancellation)?;
                    sounds.push(render(recipe, sample_rate)?);
                }
                check_cancelled(cancellation)?;
                let audio = mix(&sounds.iter().collect::<Vec<_>>())?;
                self.save(&audio, &recipes, &args, cancellation)?
            }
            _ => return Err("unknown tool".into()),
        };
        Ok(value)
    }

    fn save(
        &self,
        audio: &AudioBuffer,
        recipes: &[Recipe],
        args: &Value,
        cancellation: &CancellationToken,
    ) -> Result<Value> {
        self.save_bytes(
            audio,
            audio.wav_bytes(),
            2,
            json!({
                "sample_rate":audio.sample_rate(),"recipes":recipes
            }),
            args,
            cancellation,
        )
    }

    fn save_asset(
        &self,
        asset: &chirrp::RenderedAsset,
        args: &Value,
        cancellation: &CancellationToken,
    ) -> Result<Value> {
        self.save_bytes(
            &asset.audio,
            asset.wav_bytes(),
            asset.channels(),
            asset.sidecar.clone(),
            args,
            cancellation,
        )
    }

    fn save_bytes(
        &self,
        audio: &AudioBuffer,
        wav: Vec<u8>,
        channels: u16,
        sidecar: Value,
        args: &Value,
        cancellation: &CancellationToken,
    ) -> Result<Value> {
        // Rendering cannot be interrupted inside a DSP sample loop. Honor
        // cancellation before committing an export to the filesystem.
        check_cancelled(cancellation)?;
        let name = args.get("name").and_then(Value::as_str).unwrap_or("sound");
        if name.is_empty()
            || name.len() > 80
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(
                "name must contain 1–80 ASCII letters, digits, hyphens or underscores".into(),
            );
        }
        // Creating a fresh directory atomically prevents overwriting files or
        // following a pre-existing symlink supplied as an asset name.
        let mut suffix = 1u64;
        let directory = loop {
            let directory = self.output.join(format!("{name}-{suffix:03}"));
            match fs::create_dir(&directory) {
                Ok(()) => break directory,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => suffix += 1,
                Err(e) => return Err(e.into()),
            }
            check_cancelled(cancellation)?;
        };
        let wav_path = directory.join("sound.wav");
        let recipe_path = directory.join("recipes.json");
        let write = || -> Result<()> {
            check_cancelled(cancellation)?;
            fs::write(&wav_path, &wav)?;
            fs::write(&recipe_path, serde_json::to_vec_pretty(&sidecar)?)?;
            Ok(())
        };
        if let Err(error) = write() {
            let _ = fs::remove_file(&wav_path);
            let _ = fs::remove_file(&recipe_path);
            let _ = fs::remove_dir(&directory);
            return Err(error);
        }
        Ok(
            json!({"path":wav_path,"recipe_path":recipe_path,"format":"wav","sample_rate":audio.sample_rate(),
            "channels":channels,"frames":audio.frames(),"metrics":if channels == 1 { audio.mono_metrics() } else { audio.metrics() }}),
        )
    }
}

fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T> {
    Ok(serde_json::from_value(args[key].clone())?)
}

// Validate the schema vocabulary used by Chirrp before applying defaults or
// invoking any stateful operation. Recipe internals use Recipe::validate.
fn validate(schema: &Value, value: &mut Value, path: &str) -> Result<()> {
    let invalid = || format!("invalid argument: {path}");
    match schema["type"].as_str() {
        Some("object") => {
            let object = value.as_object_mut().ok_or_else(invalid)?;
            if let Some(required) = schema["required"].as_array() {
                for key in required {
                    let key = key.as_str().unwrap();
                    if !object.contains_key(key) {
                        return Err(format!("{path}.{key} is required").into());
                    }
                }
            }
            if schema["minProperties"]
                .as_u64()
                .is_some_and(|min| object.len() < min as usize)
            {
                return Err(invalid().into());
            }
            if let Some(properties) = schema["properties"].as_object() {
                for (key, item) in object.iter_mut() {
                    if let Some(property) = properties.get(key) {
                        validate(property, item, &format!("{path}.{key}"))?;
                    } else if schema["additionalProperties"] == false {
                        return Err(format!("unknown argument: {path}.{key}").into());
                    }
                }
                for (key, property) in properties {
                    if !object.contains_key(key)
                        && let Some(default) = property.get("default")
                    {
                        object.insert(key.clone(), default.clone());
                    }
                }
            }
        }
        Some("array") => {
            let array = value.as_array_mut().ok_or_else(invalid)?;
            if schema["minItems"]
                .as_u64()
                .is_some_and(|min| array.len() < min as usize)
                || schema["maxItems"]
                    .as_u64()
                    .is_some_and(|max| array.len() > max as usize)
            {
                return Err(invalid().into());
            }
            for (i, item) in array.iter_mut().enumerate() {
                validate(&schema["items"], item, &format!("{path}[{i}]"))?;
            }
        }
        Some("integer" | "number") => {
            let number = value.as_f64().ok_or_else(invalid)?;
            if !number.is_finite()
                || (schema["type"] == "integer" && !value.is_u64())
                || schema["minimum"].as_f64().is_some_and(|min| number < min)
                || schema["maximum"].as_f64().is_some_and(|max| number > max)
            {
                return Err(invalid().into());
            }
        }
        Some("boolean") => {
            if !value.is_boolean() {
                return Err(invalid().into());
            }
        }
        Some("string") => {
            let string = value.as_str().ok_or_else(invalid)?;
            let length = string.chars().count();
            if schema["minLength"]
                .as_u64()
                .is_some_and(|min| length < min as usize)
                || schema["maxLength"]
                    .as_u64()
                    .is_some_and(|max| length > max as usize)
                || (schema["pattern"] == "^[A-Za-z0-9_-]+$"
                    && (string.is_empty()
                        || !string
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))))
                || schema["enum"]
                    .as_array()
                    .is_some_and(|allowed| !allowed.contains(value))
            {
                return Err(invalid().into());
            }
        }
        _ => return Err("unsupported argument schema".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deadlines_release_queued_work_and_cancel_active_exports() {
        let output = std::env::temp_dir().join(format!("chirrp-deadline-{}", uuid::Uuid::new_v4()));
        let mut server = Server::new(output.clone()).unwrap();
        server.tool_timeout = Duration::from_millis(25);
        let (transport, _client) = tokio::io::duplex(8192);
        let service = rmcp::service::serve_directly(server.clone(), transport, None);
        let context =
            || RequestContext::new(rmcp::model::RequestId::Number(1), service.peer().clone());
        let request = |name: &str, arguments: Value| {
            serde_json::from_value::<CallToolRequestParams>(
                json!({"name":name,"arguments":arguments}),
            )
            .unwrap()
        };

        let guard = server.worker.lock().await;
        let result = server
            .call_tool(
                request("generate_sound", json!({"kind":"ui_click"})),
                context(),
            )
            .await
            .unwrap();
        let result = serde_json::to_value(rmcp::model::ServerResult::from(result)).unwrap();
        assert_eq!(result["isError"], true);
        assert!(result.to_string().contains("timed out while queued"));
        assert_eq!(server.capacity.available_permits(), MAX_TOOL_CALLS);
        drop(guard);

        let result = server
            .call_tool(
                request(
                    "mix_sounds",
                    json!({
                        "recipes":vec![Recipe::new(SoundKind::Thunder, 42); 32],"sample_rate":96000
                    }),
                ),
                context(),
            )
            .await
            .unwrap();
        let result = serde_json::to_value(rmcp::model::ServerResult::from(result)).unwrap();
        assert_eq!(result["isError"], true);
        assert!(result.to_string().contains("timed out;"));
        server.finish_work().await;
        assert_eq!(server.capacity.available_permits(), MAX_TOOL_CALLS);
        assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
        service.cancel().await.unwrap();
        fs::remove_dir_all(output).unwrap();
    }
}
