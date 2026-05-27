# Clipshare

Do you ever have to work on multiple machines?

Do you ever used your Github™ Gists just to send some text between then?

Clipshare is here to save the day!

You can now secure share your clipboard between your machines (well, given they are on the same network).

## How to use

### Direct TCP mode (recommended)

On one machine, start the server with a fixed port:

```bash
$ clipshare --listen --port 12345
Clipshare server listening on port 12345
```

And then on another machine on the same network:

```bash
$ clipshare --connect 192.168.0.200:12345
Connecting to 192.168.0.200:12345 (retry until Ctrl+C)...
```

### Config file mode

Create a config at `~/.config/clipshare.toml` (auto-created with defaults on first run):

```toml
server_ip = "192.168.0.200"
server_port = 12345
```

Then on the server machine:

```bash
$ clipshare --listen
```

And on a client machine:

```bash
$ clipshare
```

### Legacy UDP discovery mode

```bash
# On one machine (server)
$ clipshare
Run `clipshare 11337` on another machine of your network

# On another machine (client)
$ clipshare 11337
Connecting to clipboard 11337...
Clipboards connected
```

And voilá, the clipboards of both machines are now magically the same!

The terminal will beep each time a clipboard change is synced from a remote peer.

## Installation

### Pre-Built Binary
Each release comes with pre-built binaries of several platforms. Grab it from [Github Releases](https://github.com/WShiBin/clipshare/releases).

### Cargo
If you are a Rust enthusiast, installing via Cargo is just:
```bash
$ cargo install clipshare
```

### From source
Make sure you have Rust installed, then:
```bash
$ git clone https://github.com/WShiBin/clipshare.git
$ cd clipshare
$ cargo build --release
$ cp ./target/release/clipshare /usr/local/bin/
```

## Configuration

Clipshare reads `~/.config/clipshare.toml` when no explicit CLI flags are given. The config is auto-created with defaults on first run.

```toml
server_ip = "192.168.0.200"
server_port = 12345
```

CLI flags override the config file.

## Limitations

Yes

<sup><sub>Really, it is quite limited, it can only share utf8 encoded text and images. Unfortunately you can´t share files for now.</sub></sup>

## Implementation

Nothing fancy here, we just broadcast the internal ip on the network and connect the processes using the informed ~~port~~ "clipboard code".

The data then is transfered between the machines via an encrypted TLS connection.

In direct TCP mode (the default), clipshare skips UDP discovery and connects directly to a fixed IP:port. The server will try up to 5 consecutive ports (+0 to +4) before giving up, and the client will retry through the same port range until a connection succeeds.
