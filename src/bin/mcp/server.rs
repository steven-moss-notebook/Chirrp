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
use tokio::sync::Mutex;
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::sync::CancellationToken;

pub(super) type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub(super) const MAX_REQUEST: usize = 1024 * 1024;

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
    let service = server.serve((writer, reader)).await?;
    let cancellation = service.cancellation_token();
    let shutdown = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancellation.cancel();
        }
    });
    let result = service.waiting().await;
    shutdown.abort();
    result?;
    Ok(())
}

fn definitions() -> Result<Vec<Tool>> {
    let mut tools: Vec<_> = chirrp::tool_definitions().into_iter().map(|tool| {
        json!({"name":tool.name,"description":tool.description,"inputSchema":tool.input_schema})
    }).collect();
    let name = json!({"type":"string","description":"Optional asset directory label, using ASCII letters, digits, hyphens or underscores (1–80 characters). A numeric suffix prevents overwrites."});
    for tool in &mut tools {
        if matches!(tool["name"].as_str(), Some("render_audio" | "export_wav")) {
            tool["description"] = json!(
                "Render a candidate to a local PCM16 stereo WAV and recipe sidecar. Returns absolute paths and audio metrics; no PCM arrays. Every call creates a new asset directory."
            );
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
        "description":"Render and layer any nonempty array of recipe objects (from get_recipe or the recipes array in a saved sidecar). Saves a stereo WAV and all recipes. Sounds start together, preserve the longest tail, and share a 0.89 peak ceiling. Accepts one recipe to re-render a saved sound. Does not change the active session.",
        "inputSchema":{"type":"object","additionalProperties":false,"required":["recipes"],"properties":{
            "recipes":{"type":"array","minItems":1,"items":{"type":"object","description":"A complete Chirrp recipe object, including version, kind, seed, and genome."}},
            "sample_rate":rate,"name":name
        }}
    }));
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
            .with_instructions("Generate local game sound assets with generate_sound. Use list_sounds to discover presets. For iterative design: create_sound, edit_sound/randomize/evolve, then export_wav. Each process has one editing session. Get recipes before replacing it to mix different categories with mix_sounds. Exports return local WAV paths and recipes.json sidecars; the host handles playback. Tool operations run offline and are serialized.")
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
        // Wait asynchronously for exclusive access to the session. Moving the
        // owned guard into spawn_blocking prevents canceled tasks from releasing
        // it while a render is still running, bounding CPU and memory use.
        let mut worker = tokio::select! {
            biased;
            _ = context.ct.cancelled() => return Ok(tool_error("tool call cancelled".into()).into()),
            worker = self.worker.clone().lock_owned() => worker,
        };
        let cancellation = context.ct.clone();
        let work = tokio::task::spawn_blocking(move || {
            check_cancelled(&cancellation)?;
            worker.call(&tool.name, args, &cancellation)
        });
        let result = tokio::select! {
            biased;
            _ = context.ct.cancelled() => return Ok(tool_error("tool call cancelled".into()).into()),
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
        };
        let wav_path = directory.join("sound.wav");
        let recipe_path = directory.join("recipes.json");
        let write = || -> Result<()> {
            fs::write(&wav_path, audio.wav_bytes())?;
            fs::write(
                &recipe_path,
                serde_json::to_vec_pretty(&json!({
                    "sample_rate":audio.sample_rate(),"recipes":recipes
                }))?,
            )?;
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
            "channels":audio.channels(),"frames":audio.frames(),"metrics":audio.metrics()}),
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
        Some("string") => {
            if !value.is_string()
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
