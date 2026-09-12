//! Optional native MCP executable. SDK dependencies stay out of the library.
#[cfg(not(target_arch = "wasm32"))]
#[path = "mcp/server.rs"]
mod server;

#[cfg(not(target_arch = "wasm32"))]
#[path = "mcp/cli.rs"]
mod cli;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = cli::run() {
        eprintln!("chirrp-mcp: {error}");
        std::process::exit(1);
    }
}

// The MCP server is native-only; keep all-feature WASM library builds usable.
#[cfg(target_arch = "wasm32")]
fn main() {}
