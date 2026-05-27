use std::{error::Error, io, net::SocketAddr, process::exit, sync::Arc, time::Duration};

use clap::Parser;
use clipboard::ClipboardObject;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    select,
    time::{sleep, timeout},
};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tracing::{debug, error_span, instrument, metadata::LevelFilter, trace, Instrument};
use tracing_error::ErrorLayer;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::clipboard::Clipboard;
use config::Config;

mod clipboard;
mod config;

const HANDSHAKE: &[u8; 9] = b"clipshare";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Clipboard id to connect to (UDP discovery mode, legacy)
    clipboard: Option<u16>,

    /// Run as server (listen for connections)
    #[arg(long)]
    listen: bool,

    /// Connect to a specific address (format: IP:PORT)
    #[arg(long, value_name = "IP:PORT")]
    connect: Option<String>,

    /// Server port (only used with --listen)
    #[arg(long)]
    port: Option<u16>,

    /// Don't clear the clipboard on start
    #[arg(long)]
    no_clear: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::ERROR.into())
                .from_env_lossy(),
        )
        .with(ErrorLayer::default())
        .init();

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
                let addr: SocketAddr = addr_str.parse().unwrap_or_else(|_| {
                    eprintln!(
                        "Invalid --connect format. Use IP:PORT (e.g. 192.168.0.200:12345) \
                         or [IPv6]:PORT (e.g. [::1]:12345)"
                    );
                    exit(1);
                });
                start_client_direct(clipboard, &addr.ip().to_string(), addr.port()).await
            } else {
                // Default: client mode — read config
                let config = load_or_create_config();
                start_client_direct(clipboard, &config.server_ip, config.server_port).await
            }
        }
    }
}

#[instrument(skip(clipboard))]
async fn start_server(clipboard: Arc<Clipboard>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;
    let port = socket.local_addr()?.port();

    let rcgen::CertifiedKey { cert, signing_key } = rcgen::generate_simple_self_signed([])?;
    let public_key = cert.der().to_vec();
    let private_key = signing_key.serialize_der();

    tokio::spawn(
        async move {
            loop {
                if socket.send_to(HANDSHAKE, ("255.255.255.255", port)).await? == 0 {
                    debug!("Failed to send UDP packet");
                    break;
                }
                sleep(Duration::from_secs(3)).await;
            }
            io::Result::Ok(())
        }
        .instrument(error_span!("Port publishing", port)),
    );

    let tls_acceptor = {
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(public_key)],
                PrivateKeyDer::Pkcs8(private_key.into()),
            )?;
        TlsAcceptor::from(Arc::new(config))
    };

    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    eprintln!("Run `clipshare {port}` on another machine of your network");

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

#[instrument(skip(clipboard))]
async fn start_server_fixed(
    clipboard: Arc<Clipboard>,
    port: u16,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let rcgen::CertifiedKey { cert, signing_key } = rcgen::generate_simple_self_signed([])?;
    let public_key = cert.der().to_vec();
    let private_key = signing_key.serialize_der();

    let tls_acceptor = {
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(public_key)],
                PrivateKeyDer::Pkcs8(private_key.into()),
            )?;
        TlsAcceptor::from(Arc::new(config))
    };

    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    eprintln!("Clipshare server listening on port {port}");

    while let Ok((stream, addr)) = listener.accept().await {
        let stream = match tls_acceptor.accept(stream).await {
            Ok(s) => s,
            Err(e) => {
                debug!(error = %e, "TLS accept failed");
                continue;
            }
        };
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

#[instrument(skip(clipboard))]
async fn start_client(
    clipboard: Arc<Clipboard>,
    clipboard_port: u16,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let socket = UdpSocket::bind(("0.0.0.0", clipboard_port)).await?;
    eprintln!("Connecting to clipboard {clipboard_port}...");
    let mut buf = [0_u8; 9];

    let Ok(Ok((_, addr))) = timeout(Duration::from_secs(5), socket.recv_from(&mut buf)).await
    else {
        eprintln!("Timeout trying to connect to clipboard {clipboard_port}");
        exit(1);
    };

    if &buf == HANDSHAKE {
        let tls_connector = {
            let config = rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoCa))
                .with_no_client_auth();
            TlsConnector::from(Arc::new(config))
        };

        trace!("Begin client connection");
        let stream = TcpStream::connect(addr).await?;
        let ip = stream.peer_addr()?.ip();
        let stream = tls_connector
            .connect(ServerName::IpAddress(ip.into()), stream)
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
    } else {
        eprintln!("Clipboard {clipboard_port} not found");
        exit(1);
    }
}

#[instrument(skip(clipboard))]
async fn start_client_direct(
    clipboard: Arc<Clipboard>,
    server_ip: &str,
    server_port: u16,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    eprintln!("Connecting to clipboard at {server_ip}:{server_port}...");

    let tls_connector = {
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCa))
            .with_no_client_auth();
        TlsConnector::from(Arc::new(config))
    };

    let addr = format!("{server_ip}:{server_port}");
    let stream = timeout(Duration::from_secs(5), TcpStream::connect(&addr))
        .await
        .map_err(|_| format!("Connection to {addr} timed out after 5s"))?
        .map_err(|e| format!("Failed to connect to {addr}: {e}"))?;
    let ip = stream.peer_addr()?.ip();
    let stream = tls_connector
        .connect(ServerName::IpAddress(ip.into()), stream)
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

#[instrument(skip(clipboard, stream))]
async fn send_clipboard(
    clipboard: Arc<Clipboard>,
    mut stream: impl AsyncWrite + Send + Unpin,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    loop {
        clipboard
            .paste()
            .in_current_span()
            .await?
            .write(&mut stream)
            .in_current_span()
            .await?;
        stream.flush().await?;
    }
}

#[instrument(skip(clipboard, stream))]
async fn recv_clipboard(
    clipboard: Arc<Clipboard>,
    mut stream: impl AsyncRead + Send + Unpin,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    loop {
        let obj = ClipboardObject::from_reader(&mut stream)
            .in_current_span()
            .await?;
        clipboard.copy(obj).in_current_span().await?;
    }
}

fn load_or_create_config() -> Config {
    match Config::load() {
        Ok(config) => config,
        Err(config::ConfigError::FileNotFound(_)) => match Config::create_default() {
            Ok(config) => {
                eprintln!("Created default config at ~/.clipshare.toml");
                config
            }
            Err(e) => {
                eprintln!("Failed to create default config: {e}");
                exit(1);
            }
        },
        Err(e) => {
            eprintln!("Failed to load config: {e}");
            exit(1);
        }
    }
}

#[derive(Debug)]
struct NoCa;

impl ServerCertVerifier for NoCa {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}
