# Clipshare Fixed IP Connection Design

## Background

Clipshare currently uses UDP broadcast for service discovery: the server picks a random port, broadcasts it via UDP, and the client discovers it by listening on that port. This works well on dynamic local networks but is cumbersome in company environments where both machines have fixed IPs — the user must type a random port number every time.

## Goal

Allow clipshare to work with fixed IPs via a configuration file, so that:
- The client can connect with zero arguments (`clipshare` and done)
- The server starts with a single `--listen` flag
- Configuration is persisted in `~/.clipshare.toml` and auto-created with defaults
- Backward compatibility with the existing UDP discovery mode is preserved

## Non-Goals

- No multi-server or server selection UX
- No encryption key management (TLS self-signed remains unchanged)
- No systemd service or auto-start (user manages that)

## Configuration File

**Path**: `~/.clipshare.toml`

**Schema**:
```toml
server_ip = "192.168.0.200"
server_port = 12345
```

- Auto-created with the above defaults if the file does not exist on startup
- If parsing fails, the program prints the error and exits (no silent fallback)
- Only one server target — simple, matches the single-server use case

## CLI Changes

| Command | Mode | Behavior |
|---------|------|----------|
| `clipshare` | **Client** (default) | Reads config, connects to `server_ip:server_port` directly via TCP+TLS, skipping UDP discovery |
| `clipshare --listen` | Server | Reads config port, binds `0.0.0.0:port`, no UDP broadcast |
| `clipshare --listen --port 54321` | Server | Explicit port override |
| `clipshare --connect 1.2.3.4:5678` | Client | Explicit address override, skips config |
| `clipshare <port>` | Client (legacy) | UDP discovery mode, backward compatible |

## Data Flow

### Server Mode (`clipshare --listen`)

```
1. Parse args → --listen detected
2. Read config → server_port = 12345 (or --port override)
3. Generate self-signed TLS cert (existing)
4. Bind TCP listener on 0.0.0.0:server_port
5. Print "Clipshare server listening on port 12345"
6. Accept connections (existing TLS + clipboard sync loop)
```

No UDP broadcast is sent. The server waits passively for direct TCP connections.

### Client Mode (`clipshare` — default)

```
1. Parse args → no subcommand / positional
2. Read config → server_ip = 192.168.0.200, server_port = 12345
3. Connect TCP to server_ip:server_port
4. Send "clipshare" handshake bytes
5. Establish TLS connection (existing NoCa verifier)
6. Enter clipboard sync loop (existing)
```

The entire UDP discovery phase is skipped.

## Dependency Changes

Add to `Cargo.toml`:
- `serde` (with `derive` feature) — config deserialization
- `toml` — TOML parsing
- `dirs` — cross-platform home directory resolution

## Backward Compatibility

- `clipshare <port>` still uses UDP broadcast discovery exactly as before
- Servers started without `--listen` (current behavior via bare `clipshare`) will change: bare `clipshare` now means client mode. This is a deliberate breaking change for the better UX tradeoff. Users who need the old server behavior add `--listen`
- The config file path and format are new — no migration needed

## Error Handling

- Config file not found: auto-create with defaults, proceed
- Config file parse error: print error + exit(1)
- `--connect` with unreachable host: print error + exit(1) (same as current timeout behavior)
- No config and no `--listen`/`--connect`/port: reads config, which was auto-created, so this path always has values

## Files to Modify

- `src/main.rs` — CLI args (`--listen`, `--connect`, `--port`), config loading, role selection logic
- `Cargo.toml` — add `serde`, `toml`, `dirs` dependencies

No changes to `clipboard.rs` are required — the clipboard sync loop is unchanged.
