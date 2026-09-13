//! Installed CLI and locally managed Streamable HTTP server.
use super::security;
use super::server::{MAX_REQUEST, Result, Server, serve_stdio};
use axum::{Router, extract::Request, http::StatusCode, routing::post};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const DEFAULT_PORT: u16 = 8765;
const TIMEOUT: Duration = Duration::from_secs(15);
const HELP: &str = "Usage:
  chirrp-mcp start --output-folder <folder> [--port <port>]
  chirrp-mcp stop
  chirrp-mcp stdio [--output-folder <folder>]

start launches a background MCP server at http://127.0.0.1:8765/mcp.
stop shuts down the background server for this user.
stdio runs in the foreground for agents that launch their own subprocess.
Output defaults to ./chirrp-output; --output-dir is a compatibility alias.
Set CHIRRP_MCP_STATE_DIR to manage a separate instance (use a different port).
Set CHIRRP_MCP_AUTH_TOKEN to require a bearer token for HTTP (32–256 characters).
Authentication is optional, local-only, and does not apply to stdio or stop.
Cargo install: cargo install --path . --locked --features mcp --bin chirrp-mcp";

#[derive(Serialize, Deserialize)]
struct Instance {
    port: u16,
    token: String,
}

pub fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    if args == ["--version"] {
        println!("chirrp-mcp {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    // Preserve the original no-subcommand stdio interface.
    let (command, options) = match args.first() {
        Some(arg) if !arg.starts_with('-') => (arg.as_str(), &args[1..]),
        _ => ("stdio", args.as_slice()),
    };
    if command == "stop" {
        if !options.is_empty() {
            return Err("stop takes no arguments".into());
        }
        return stop();
    }
    if !matches!(command, "start" | "stdio" | "__serve-http") {
        return Err(format!("unknown command: {command}\n{HELP}").into());
    }
    let mut output = PathBuf::from("chirrp-output");
    let mut port = DEFAULT_PORT;
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match option.as_str() {
            "--output-folder" | "--output-dir" => {
                output = options
                    .next()
                    .ok_or("output option requires a folder")?
                    .into();
            }
            "--port" if command != "stdio" => {
                port = options.next().ok_or("--port requires a number")?.parse()?;
            }
            _ => return Err(format!("unknown option: {option}").into()),
        }
    }
    if command == "start" {
        security::auth_token()?;
        return start(&output, port);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        if command == "stdio" {
            serve_stdio(output).await
        } else {
            serve_http(output, port).await
        }
    });
    // Stdin may remain blocked after Ctrl+C in stdio mode.
    runtime.shutdown_timeout(Duration::from_secs(2));
    result
}

fn state_dir() -> Result<PathBuf> {
    let path = if let Some(path) = std::env::var_os("CHIRRP_MCP_STATE_DIR") {
        PathBuf::from(path)
    } else {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .ok_or("set CHIRRP_MCP_STATE_DIR: home directory unavailable")?;
        PathBuf::from(home).join(".chirrp").join("mcp")
    };
    fs::create_dir_all(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(path)
}

fn private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

fn start(output: &Path, port: u16) -> Result<()> {
    let state = state_dir()?;
    let log_path = state.join("server.log");
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("__serve-http")
        .arg("--output-folder")
        .arg(output)
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000008 | 0x00000200); // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().ok_or("child stdout unavailable")?;
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = send.send(result);
    });
    match receive.recv_timeout(TIMEOUT) {
        Ok(Ok(line)) if line.starts_with("READY ") => {
            println!(
                "Chirrp MCP started: {}",
                line.trim_start_matches("READY ").trim()
            );
            println!("Assets: {}", output.canonicalize()?.display());
            println!("Stop with: chirrp-mcp stop");
            Ok(())
        }
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            Err(format!("server failed to start; see {} (it may already be running or the port may be occupied)", log_path.display()).into())
        }
    }
}

fn stop() -> Result<()> {
    let state = state_dir()?;
    let lock = private_file(&state.join("server.lock"))?;
    match lock.try_lock() {
        Ok(()) => {
            // A crashed process releases the OS lock; never signal a saved PID.
            let _ = fs::remove_file(state.join("server.json"));
            println!("Chirrp MCP is not running.");
            return Ok(());
        }
        Err(TryLockError::WouldBlock) => {}
        Err(error) => return Err(error.into()),
    }
    let instance: Instance = serde_json::from_reader(File::open(state.join("server.json"))?)?;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, instance.port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    write!(
        stream,
        "POST /_shutdown HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        instance.token
    )?;
    let mut response = String::new();
    stream.take(4096).read_to_string(&mut response)?;
    if !response.starts_with("HTTP/1.1 202") {
        return Err("server rejected shutdown; state may belong to an older instance".into());
    }
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(TryLockError::WouldBlock) => {
                return Err("shutdown requested; server is still finishing work".into());
            }
            Err(error) => return Err(error.into()),
        }
    }
    println!("Chirrp MCP stopped.");
    Ok(())
}

async fn serve_http(output: PathBuf, port: u16) -> Result<()> {
    let auth_token = security::auth_token()?;
    let state = state_dir()?;
    // Keep the file (and therefore the cross-process lock) open until shutdown.
    let lock = private_file(&state.join("server.lock"))?;
    lock.try_lock()
        .map_err(|error| format!("another server is running or starting: {error}"))?;
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
    let address = listener.local_addr()?;
    let server = Server::new(output)?;
    let cancellation = CancellationToken::new();
    let mut config = StreamableHttpServerConfig::default();
    config.cancellation_token = cancellation.clone();
    config.max_request_body_bytes = MAX_REQUEST;
    config.allowed_hosts = vec![address.to_string(), format!("localhost:{}", address.port())];
    config.allowed_origins = vec![
        format!("http://{address}"),
        format!("http://localhost:{}", address.port()),
    ];
    config.json_response = true;
    let factory = server.clone();
    let service: StreamableHttpService<Server, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(factory.clone()), Default::default(), config);
    let instance = Instance {
        port: address.port(),
        token: uuid::Uuid::new_v4().to_string(),
    };
    let shutdown_token = instance.token.clone();
    let shutdown = cancellation.clone();
    let router = Router::new()
        .nest_service("/mcp", service)
        .route(
            "/_shutdown",
            post(move |request: Request| {
                let authorized = security::authorized(request.headers(), &shutdown_token)
                    && request.headers().get("host").and_then(|h| h.to_str().ok())
                        == Some(address.to_string().as_str())
                    && !request.headers().contains_key("origin");
                let shutdown = shutdown.clone();
                async move {
                    if !authorized {
                        return StatusCode::FORBIDDEN;
                    }
                    shutdown.cancel();
                    StatusCode::ACCEPTED
                }
            }),
        )
        .layer(axum::middleware::from_fn(move |request, next| {
            security::protect(request, next, auth_token.clone())
        }));
    let mut record = private_file(&state.join("server.json"))?;
    record.set_len(0)?;
    serde_json::to_writer(&mut record, &instance)?;
    record.sync_all()?;
    drop(record);
    println!("READY http://{address}/mcp");
    std::io::stdout().flush()?;
    let signal = cancellation.clone();
    let signals = tokio::spawn(async move {
        #[cfg(unix)]
        {
            if let Ok(mut terminate) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            } else {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
        #[cfg(not(unix))]
        let _ = tokio::signal::ctrl_c().await;
        signal.cancel();
    });
    let shutdown = cancellation.clone();
    let serving = axum::serve(listener, router).with_graceful_shutdown(async move {
        shutdown.cancelled().await;
    });
    let serving = std::future::IntoFuture::into_future(serving);
    tokio::pin!(serving);
    let result = tokio::select! {
        result = &mut serving => result,
        _ = cancellation.cancelled() => {
            // Bound shutdown even if a client keeps an idle HTTP connection open.
            match tokio::time::timeout(Duration::from_secs(2), &mut serving).await {
                Ok(result) => result,
                Err(_) => Ok(()),
            }
        }
    };
    cancellation.cancel();
    server.finish_work().await;
    signals.abort();
    let _ = fs::remove_file(state.join("server.json"));
    result?;
    Ok(())
}
