# Fixed IP Connection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `~/.clipshare.toml` config + `--listen`/`--connect`/`--port` CLI args so clipshare works with fixed IPs without daily port entry.

**Architecture:** Config module reads/writes `~/.clipshare.toml` (auto-create with defaults). CLI parser gains `--listen` (server), `--connect <IP:PORT>` (client override), `--port <PORT>` (server port override). `clipshare` defaults to client mode. `clipshare <port>` keeps UDP discovery for backward compat. No changes to clipboard sync loop.

**Tech Stack:** Rust, Tokio, rustls, clap. Adds `serde`, `toml`, `dirs` crates.

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `Cargo.toml` | Modify | Add serde, toml, dirs deps |
| `src/config.rs` | Create | Config struct, load/create_default, error types |
| `src/main.rs` | Modify | New CLI args, role selection, direct connect/server |

No changes to `src/clipboard.rs`.

---

### Task 1: Add dependencies to Cargo.toml

**Files:**
- Modify: `Cargo.toml` (lines 12-21)

- [ ] **Step 1: Add serde, toml, dirs entries**

Insert after the existing `tokio-rustls` line:

```toml
serde = { version = "1", features = ["derive"] }
toml = "0.8"
dirs = "5"
```

- [ ] **Step 2: Verify dependencies resolve**

Run: `cargo check`

Expected output: `Compiling clipshare v0.0.7` followed by no errors (or warnings from unused deps which will be resolved in later tasks).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add serde, toml, dirs dependencies"
```

---

### Task 2: Create config module

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs` (add `mod config;`)

- [ ] **Step 1: Create src/config.rs**

```rust
use std::path::PathBuf;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub server_ip: String,
    pub server_port: u16,
}

#[derive(Debug)]
pub enum ConfigError {
    NoHomeDir,
    FileNotFound(PathBuf),
    Parse(PathBuf, String),
    Serialize(String),
    Io(PathBuf, String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NoHomeDir => write!(f, "cannot determine home directory"),
            ConfigError::FileNotFound(path) => write!(f, "config file not found: {}", path.display()),
            ConfigError::Parse(path, e) => write!(f, "failed to parse config {}: {e}", path.display()),
            ConfigError::Serialize(e) => write!(f, "failed to serialize config: {e}"),
            ConfigError::Io(path, e) => write!(f, "IO error on {}: {e}", path.display()),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn load() -> Result<Config, ConfigError> {
        let path = config_path()?;
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                toml::from_str(&content)
                    .map_err(|e| ConfigError::Parse(path, e.to_string()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(ConfigError::FileNotFound(path))
            }
            Err(e) => Err(ConfigError::Io(path, e.to_string())),
        }
    }

    pub fn create_default() -> Result<Config, ConfigError> {
        let config = Config {
            server_ip: "192.168.0.200".to_string(),
            server_port: 12345,
        };
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ConfigError::Io(path.clone(), e.to_string()))?;
        }
        let content = toml::to_string(&config)
            .map_err(|e| ConfigError::Serialize(e.to_string()))?;
        std::fs::write(&path, &content)
            .map_err(|e| ConfigError::Io(path, e.to_string()))?;
        Ok(config)
    }

    #[cfg(test)]
    pub fn test_default() -> Config {
        Config {
            server_ip: "192.168.0.200".to_string(),
            server_port: 12345,
        }
    }
}

fn config_path() -> Result<PathBuf, ConfigError> {
    dirs::home_dir()
        .map(|h| h.join(".clipshare.toml"))
        .ok_or(ConfigError::NoHomeDir)
}
```

- [ ] **Step 2: Add `mod config;` to src/main.rs**

Insert at line 18 (after the existing `mod clipboard;`):

```rust
mod config;
```

- [ ] **Step 3: Write config unit tests**

Append to `src/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_config() {
        let toml_str = r#"
server_ip = "192.168.1.100"
server_port = 54321
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server_ip, "192.168.1.100");
        assert_eq!(config.server_port, 54321);
    }

    #[test]
    fn parses_default_values() {
        let config = Config::test_default();
        assert_eq!(config.server_ip, "192.168.0.200");
        assert_eq!(config.server_port, 12345);
    }

    #[test]
    fn serializes_and_deserializes() {
        let original = Config::test_default();
        let toml_str = toml::to_string(&original).unwrap();
        let restored: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(original.server_ip, restored.server_ip);
        assert_eq!(original.server_port, restored.server_port);
    }

    #[test]
    fn rejects_invalid_toml() {
        let result: Result<Config, toml::de::Error> = toml::from_str("not valid toml {{{");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_missing_fields() {
        let result: Result<Config, toml::de::Error> = toml::from_str("server_ip = \"1.2.3.4\"");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 4: Run config tests**

Run: `cargo test config::tests -- --nocapture`

Expected: all 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "feat: add config module for ~/.clipshare.toml"
```

---

### Task 3: Update CLI arguments

**Files:**
- Modify: `src/main.rs` (lines 23-31, the `Cli` struct)

- [ ] **Step 1: Add --listen, --connect, --port to Cli struct**

Replace the existing `Cli` struct:

```rust
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Clipboard id to connect to (UDP discovery mode, legacy)
    clipboard: Option<u16>,

    /// Run as server (listen for connections)
    #[arg(long)]
    listen: bool,

    /// Connect to a specific address (format: IP:PORT)
    #[arg(long)]
    connect: Option<String>,

    /// Server port (only used with --listen)
    #[arg(long)]
    port: Option<u16>,

    /// Don't clear the clipboard on start
    #[arg(long)]
    no_clear: bool,
}
```

- [ ] **Step 2: Run cargo check**

Run: `cargo check`

Expected: compiles (warning about unused is fine).

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: add --listen, --connect, --port CLI arguments"
```

---

### Task 4: Add server fixed-port mode and direct client connect

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add server_fixed function (no UDP broadcast)**

Insert a new function after `start_server` (before `start_client` or at another logical place):

```rust
#[instrument(skip(clipboard))]
async fn start_server_fixed(
    clipboard: Arc<Clipboard>,
    port: u16,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let cert = rcgen::generate_simple_self_signed([])?;
    let public_key = cert.serialize_der()?;
    let private_key = cert.serialize_private_key_der();

    let tls_acceptor = {
        let config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(vec![Certificate(public_key)], PrivateKey(private_key))?;
        TlsAcceptor::from(Arc::new(config))
    };

    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    eprintln!("Clipshare server listening on port {port}");

    while let Ok((stream, addr)) = listener.accept().await {
        let stream = tls_acceptor.accept(stream).await?;
        trace!("New connection arrived");
        let ip = addr.ip();
        let clipboard = clipboard.clone();
        tokio::spawn(
            async move {
                let (reader, writer) = tokio::io::split(stream);

                if let Err(err) = select! {
                    result = recv_clipboard(clipboard.clone(), reader) => result,
                    result = send_clipboard(clipboard.clone(), writer) => result,
                } {
                    debug!(error = %err, "Server error");
                }
                trace!("Finishing server connection");
                Ok::<_, Box<dyn Error + Send + Sync>>(())
            }
            .instrument(error_span!("Connection", %ip)),
        );
    }

    Ok(())
}
```

- [ ] **Step 2: Add start_client_direct function (skip UDP discovery)**

Insert after `start_client`:

```rust
#[instrument(skip(clipboard))]
async fn start_client_direct(
    clipboard: Arc<Clipboard>,
    server_ip: &str,
    server_port: u16,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    eprintln!("Connecting to clipboard at {server_ip}:{server_port}...");

    let tls_connector = {
        let config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(NoCa))
            .with_no_client_auth();
        TlsConnector::from(Arc::new(config))
    };

    let addr = format!("{server_ip}:{server_port}");
    let stream = TcpStream::connect(&addr).await
        .map_err(|e| format!("Failed to connect to {addr}: {e}"))?;
    let ip = stream.peer_addr()?.ip();
    let stream = tls_connector
        .connect(ServerName::IpAddress(ip), stream)
        .await?;

    let (reader, writer) = tokio::io::split(stream);
    let span = error_span!("Connection", %ip).entered();
    eprintln!("Clipboards connected");

    if let Err(err) = select! {
        result = recv_clipboard(clipboard.clone(), reader).in_current_span() => result,
        result = send_clipboard(clipboard.clone(), writer).in_current_span() => result,
    } {
        debug!(error = %err, "Client error");
    }

    trace!("Finish client connection");
    span.exit();
    eprintln!("Clipboard closed");
    Ok(())
}
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check`

Expected: compiles without errors. New functions are not yet called so there may be dead code warnings.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add start_server_fixed and start_client_direct functions"
```

---

### Task 5: Wire main function with config + new args

**Files:**
- Modify: `src/main.rs` (the main function, lines 46-57)

- [ ] **Step 1: Add import for Config**

Add at the top of main.rs, after `mod config;`:

```rust
use config::Config;
```

(Verify `process::exit` is already imported in `use std::{...}` — it is on line 2.)

- [ ] **Step 2: Replace main function body**

Replace the existing `main` function after `let args = Cli::parse();` and `let clipboard = ...`:

```rust
    let args = Cli::parse();

    let clipboard = Arc::new(if args.no_clear {
        Clipboard::new()
    } else {
        Clipboard::cleared()
    });

    match args.clipboard {
        Some(port) => start_client(clipboard, port).await,
        None => {
            if args.listen {
                // Server mode — fixed port from config or --port
                let config = load_or_create_config();
                let port = args.port.unwrap_or(config.server_port);
                start_server_fixed(clipboard, port).await
            } else if let Some(ref addr_str) = args.connect {
                // Client mode — explicit address via --connect
                let (ip, port_str) = addr_str.split_once(':')
                    .unwrap_or_else(|| {
                        eprintln!("Invalid --connect format. Use IP:PORT (e.g. 192.168.0.200:12345)");
                        exit(1);
                    });
                let port: u16 = port_str.parse().unwrap_or_else(|_| {
                    eprintln!("Invalid port in --connect: {port_str}");
                    exit(1);
                });
                start_client_direct(clipboard, ip, port).await
            } else {
                // Default: client mode — read config
                let config = load_or_create_config();
                start_client_direct(clipboard, &config.server_ip, config.server_port).await
            }
        }
    }
```

Add `load_or_create_config` helper function near `NoCa`:

```rust
fn load_or_create_config() -> Config {
    match Config::load() {
        Ok(config) => config,
        Err(config::ConfigError::FileNotFound(_)) => {
            match Config::create_default() {
                Ok(config) => {
                    eprintln!("Created default config at ~/.clipshare.toml");
                    config
                }
                Err(e) => {
                    eprintln!("Failed to create default config: {e}");
                    exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to load config: {e}");
            exit(1);
        }
    }
}
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check`

Expected: compiles cleanly, no warnings. This is the first time all new code is wired together.

- [ ] **Step 4: Run full test suite**

Run: `cargo test`

Expected: all config tests PASS (there are no other tests).

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire config and CLI args into main — default client mode, --listen for server"
```

---

### Task 6: Verify with build

**Files:**
- None

- [ ] **Step 1: Run cargo build --release**

Run: `cargo build --release 2>&1`

Expected: builds successfully, binary at `target/release/clipshare`.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`

Expected: no warnings or errors.

- [ ] **Step 3: Run cargo fmt check**

Run: `cargo fmt --check`

Expected: no formatting issues. If there are, run `cargo fmt` and amend.

- [ ] **Step 4: Commit**

```bash
git commit -m "chore: final formatting and clippy fixes before release" src/main.rs src/config.rs
```

(Or nothing to commit if no changes needed.)
