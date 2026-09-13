# Local MCP server

`chirrp-mcp` lets an MCP-compatible agent generate game sound assets locally. It runs as a background Streamable HTTP server on loopback, or over standard input/output, and writes stereo PCM16 WAV files with reproducible recipe sidecars. Synthesis needs no audio device or external network calls. The server is an optional native executable; the library and WASM builds do not require it.

## Install with Cargo

From a published release containing the MCP server:

```sh
cargo install chirrp --locked --features mcp --bin chirrp-mcp
```

This requires a crates.io release with the `mcp` feature. To install the current checkout before those changes are published, run from the repository root:

```sh
cargo install --path . --locked --features mcp --bin chirrp-mcp
chirrp-mcp --help
```

The feature is opt-in: installing this executable is separate from adding `chirrp` as a library dependency. Neither the Rust library nor WASM bindings need the server.

The implementation uses the official Rust SDK, `rmcp` 3.3.0, with its server, stdio, and Streamable HTTP features enabled. `rmcp`, Tokio, Axum, UUID generation, and the framing helpers are optional native dependencies behind `mcp`. The Rust library's source does not reference them, and WASM builds exclude them even with all features enabled. Cargo dependencies are scoped to packages rather than individual binaries: explicitly enabling `mcp` on a native library build activates those dependencies too. Leave it disabled for ordinary library use.

Cargo places the executable in `$CARGO_HOME/bin`, normally `~/.cargo/bin` (`%USERPROFILE%\.cargo\bin` on Windows). Ensure that directory is on `PATH`, or configure the agent with the executable's absolute path. Windows uses `chirrp-mcp.exe`.

The Cargo install configuration lives in the root `Cargo.toml`:

```toml
[features]
default = []
mcp = ["dep:rmcp", "dep:tokio", "dep:tokio-util", "dep:futures-util", "dep:axum", "dep:uuid", "dep:subtle"]

[[bin]]
name = "chirrp-mcp"
path = "src/bin/chirrp-mcp.rs"
required-features = ["mcp"]
```

`cargo install` builds that target and copies it into Cargo's binary directory. There is no separate installation script. For a build without installation, use `cargo build --release --locked --features mcp --bin chirrp-mcp` and replace `chirrp-mcp` below with `./target/release/chirrp-mcp`.

## Start, connect, and stop

```sh
chirrp-mcp start --output-folder /absolute/path/to/game/assets/audio
```

`start` launches a background process and returns once it is ready. Add a **Streamable HTTP** MCP server in your agent with URL `http://127.0.0.1:8765/mcp`. Clients that accept URL entries in their `mcpServers` configuration can use:

```json
{
  "mcpServers": {
    "chirrp": { "url": "http://127.0.0.1:8765/mcp" }
  }
}
```

Client-specific configuration formats vary; use the same URL in the client's HTTP MCP setup form. Reconnect after saving configuration. To stop the background process from any working directory:

```sh
chirrp-mcp stop
```

`stop` waits for shutdown; calling it when no instance is running succeeds. The server does not restart automatically on login or reboot. It binds only to `127.0.0.1`. `start --port 8766 --output-folder ...` selects another port (`0` lets the OS choose one, printed on startup). Omit `--output-folder` to use `./chirrp-output`. Relative output paths are resolved from the directory where you run `start`.

One background instance is managed per user by default. Its lock, shutdown credentials, and `server.log` live in `~/.chirrp/mcp` (`%USERPROFILE%\.chirrp\mcp` on Windows). Set `CHIRRP_MCP_STATE_DIR` to a dedicated directory to manage another instance, and use a different port; use the same environment value for its `start` and `stop` commands. This directory is private to your user on Unix. OS file locks prevent duplicate instances and are released automatically after a crash. Stopping authenticates to the recorded instance instead of sending a signal to a saved PID. A subsequent start replaces stale state.

The HTTP transport validates Host and Origin headers and limits request bodies to 1 MiB. Browser origins other than the local endpoint are rejected. The server is intended for trusted local agents; it provides no remote access or public hosting mode. All connections to one background process share one serialized editing session. For independent editing sessions, use separate instances or stdio processes.

## Optional HTTP authentication

Set `CHIRRP_MCP_AUTH_TOKEN` before starting the background server to require `Authorization: Bearer <token>` on every MCP HTTP request, including discovery, initialization, SSE, and session deletion. With the variable unset, trusted local clients can connect as before. An empty or invalid value fails startup instead of silently disabling authentication. The token must contain 32–256 ASCII letters, digits, or `-._~+/=` characters. Generate a random secret, for example with `openssl rand -hex 32`, and store it in your client or secret manager.

In a POSIX shell, read the token without putting its value in shell history or command arguments:

```sh
read -r CHIRRP_MCP_AUTH_TOKEN
export CHIRRP_MCP_AUTH_TOKEN
chirrp-mcp start --output-folder /absolute/path/to/game/assets/audio
unset CHIRRP_MCP_AUTH_TOKEN
```

Configure your client's bearer-token setting or custom `Authorization` header with the same secret. Chirrp does not accept tokens in URLs. Secrets are not printed or stored in `server.json` or `server.log`; the background process inherits the token through its environment. Processes with permission to inspect that environment may still access it. Invalid credentials return HTTP 401 with a `WWW-Authenticate: Bearer` challenge. Comparison uses the `subtle` crate's constant-time equality, duplicate authorization headers are rejected, and responses include `Cache-Control: no-store`.

Rotate the token by stopping the server, changing the environment and client configuration, then starting it again. `stop` uses a separate random per-instance shutdown secret and does not need the MCP token. Stdio ignores this variable: the parent process and OS permissions control access.

This is pre-shared-token access control for a single-user loopback service, not MCP OAuth authorization. It does not provide user identities, scopes, per-client editing sessions, or TLS. Clients must support a configured bearer token; automatic OAuth discovery/login is not available. Keep the service local; public hosting would require a separate OAuth/TLS deployment design.

## Agent-managed stdio alternative

If the agent launches and stops its own MCP subprocess, choose **stdio** and configure:

```json
{
  "mcpServers": {
    "chirrp": {
      "command": "/absolute/path/to/.cargo/bin/chirrp-mcp",
      "args": ["stdio", "--output-folder", "/absolute/path/to/game/assets/audio"]
    }
  }
}
```

Replace both paths with locations on your machine; Windows uses `chirrp-mcp.exe`. An absolute executable path avoids relying on the agent inheriting your shell's `PATH`. Do not configure `start` as a stdio command: it launches HTTP and then exits. `stop` controls the background instance only; the agent owns its stdio processes. For compatibility, omitting the subcommand still uses stdio, and `--output-dir` remains an alias for `--output-folder`.

The agent host controls tool approval and file access and supplies its own model connection and playback support. Chirrp performs synthesis locally without API keys or external network calls.

Try this request:

> Use Chirrp to make a short laser shot, a heavy impact, and a quiet UI hover for my game. Generate them at 48 kHz, with little reverb. Save each WAV, then layer the laser and impact into a fourth sound. Return the asset paths.

## Tools and workflow

The server exposes 14 tools:

| Tool | Purpose |
| --- | --- |
| `list_sounds` | Discover the 39 presets and descriptions. |
| `generate_sound` | Generate and export a preset in one call, with optional edits. |
| `create_sound`, `random_sound` | Start an editing session with 2–12 candidates. |
| `list_candidates`, `select_candidate` | Inspect candidates and choose a favorite. |
| `edit_sound` | Adjust pitch, envelope, brightness, noise, width, room, drive, or texture. |
| `randomize`, `evolve` | Explore seeded variations or breed candidates using ratings. |
| `get_recipe` | Retrieve a complete reproducible recipe object. |
| `analyze` | Measure duration, peak, RMS, and stereo correlation. |
| `render_audio`, `export_wav` | Export a session candidate as a local WAV and recipe sidecar. |
| `mix_sounds` | Render and mix a nonempty array of complete recipe objects. |

For example, call `generate_sound` with:

```json
{
  "kind": "laser",
  "seed": 42,
  "edits": {"room": 0.1, "decay_seconds": 0.15},
  "sample_rate": 48000,
  "name": "player_laser"
}
```

The result contains `path`, `recipe_path`, `format`, `sample_rate`, `channels`, `frames`, and `metrics`. It references files such as:

```text
/your/game/assets/audio/player_laser-001/sound.wav
/your/game/assets/audio/player_laser-001/recipes.json
```

The sidecar contains `sample_rate` and a `recipes` array. To reproduce the exact WAV on the same target, pass those fields to `mix_sounds`. To combine different presets, collect recipe objects with `get_recipe` before replacing the editing session, or read the saved sidecars using the agent's filesystem tools. Pass all recipe objects together to `mix_sounds`. Mixing aligns their starts, retains stereo and the longest tail, and attenuates peaks above 0.89. The MCP adapter accepts 1–32 layers per call to bound memory and CPU use; the underlying Rust mixing API is unchanged.

For iterative design, use `create_sound`, edit or randomize, select the preferred candidate, then `export_wav` with its index. `generate_sound` and `mix_sounds` leave the active session unchanged. Default seeds are 42 and the default sample rate is 48,000 Hz. Seeds and recipes are deterministic on the same target; different architectures can have floating-point differences.

Exports always create a fresh directory with a numeric suffix, including across restarts. Existing files are never overwritten. Labels accept 1–80 ASCII letters, digits, hyphens, and underscores. Audio is returned as file references rather than large JSON sample arrays. Both `render_audio` and `export_wav` export WAV in this MCP adapter; playback or listening requires support from the agent host.

## Protocol and verification

The `rmcp` SDK handles framing, protocol negotiation, request dispatch, and cancellation. It supports legacy initialization and the current `server/discover` lifecycle, plus `tools/list` and `tools/call`; `ping` is available for legacy protocol connections. In stdio mode, stdout contains only newline-delimited JSON-RPC; diagnostics go to stderr. SDK framing limits requests to 1 MiB; malformed or oversized frames close the connection. Stdio closes on stdin EOF or Ctrl+C. The HTTP server stops through `chirrp-mcp stop`, Ctrl+C, or SIGTERM (Unix), finishing active rendering before releasing its instance lock. Background diagnostics go to `server.log`. It advertises only tool capabilities.

Tool definitions include read-only, destructive, idempotent, and open-world annotations. Preset listing and export tools publish output schemas. Successful tools return structured content alongside their text results; the structured result for `list_sounds` is an object with a `sounds` array. Invalid tool arguments and synthesis errors return visible tool errors. Unknown tools return a protocol error. Export labels and mix limits are checked before rendering or changing state.

Rendering and file writes run on a blocking worker while the async runtime continues handling protocol messages. A mutex serializes tool operations and protects the editing session. Cancellation stops queued work, is checked between rendered recipes, and prevents an export from starting after cancellation is observed. An individual DSP render already in progress finishes before its worker is released; state changes or file writes already committed are not rolled back.

At most 32 tool calls may be outstanding per process, including active work. Excess calls receive a visible `server busy` tool error and may be retried later. Calls have a 120-second deadline including queue time; expiry cancels queued work or signals the blocking worker. Protocol operations such as ping remain independent of this tool queue. After a timeout or cancellation during an export, check the output folder for the named asset before retrying: exporting is intentionally not idempotent. These limits do not impose a disk quota; manage generated files with your normal asset workflow.

```sh
cargo test --release --locked --features mcp --test mcp --test mcp_lifecycle
```

These tests launch the real server process, exercise both SDK lifecycles, inspect tool annotations and structured results, generate and mix WAV files, reproduce saved recipes, and check validation, cancellation, overload rejection, protocol responsiveness during rendering, bounded framing, and non-overwriting exports.

Lifecycle tests also start the background HTTP server, generate assets through MCP, verify editing state across requests, reject duplicate starts and invalid origins, stop it, and restart it. They exercise optional bearer authentication, invalid configuration, HTTP methods, challenge headers, credential separation, and secret redaction. These tests require permission to bind local loopback ports.
