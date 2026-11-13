//! The agent's long-lived connection to the control plane.
//!
//! The agent dials out (never the other way round) over mutual TLS 1.3 with
//! the certificate it got at enrolment, upgrades to a WebSocket and says
//! `Hello`. It then sits on the connection answering pings, and reconnects with
//! exponential backoff whenever the connection ends.
//!
//! The only server it trusts is one whose certificate was issued by the CA it
//! pinned at enrolment (`ca.crt`), under the name in the stream address.

use std::{sync::Arc, time::Duration};

use dhpanel_node_protocol::{
    AGENT_SILENCE_TIMEOUT_SECS, AgentError, AgentMessage, ControlMessage, ErrorCode, Hello,
    MAX_FRAME_BYTES, Operation, OperationResult, Output, PROTOCOL_VERSION, Parsed, STREAM_PATH,
    decode, encode, parse_operation_frame,
};
use futures_util::{SinkExt as _, StreamExt as _};
use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject as _},
    version::TLS13,
};
use secrecy::{ExposeSecret as _, SecretString};
use tokio::{
    net::TcpStream,
    sync::watch,
    time::{Instant, timeout},
};
use tokio_rustls::TlsConnector;
use tokio_tungstenite::{
    client_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

/// Longest we wait for TCP, TLS, the upgrade and `Welcome` together.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// First retry delay, and the ceiling it grows to.
const BACKOFF_START: Duration = Duration::from_secs(1);
const BACKOFF_CAP: Duration = Duration::from_secs(60);

/// Why a stream address is unusable.
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    /// The address is not `https://host[:port]`.
    #[error("invalid stream address `{0}`: expected https://host[:port]")]
    BadAddress(String),
    /// The pinned CA, the certificate or the key could not be used.
    #[error("TLS setup failed: {0}")]
    Tls(String),
}

/// Whether `url` is an acceptable stream address: `https://host[:port]`, host a
/// DNS name or IPv4 address, nothing else (no path, query, user-info, quotes).
#[must_use]
pub fn valid_stream_url(url: &str) -> bool {
    split_address(url).is_some()
}

/// `https://host[:port]` into host and port (443 by default).
fn split_address(url: &str) -> Option<(String, u16)> {
    let rest = url.strip_prefix("https://")?;
    if rest.is_empty() || rest.len() > 200 {
        return None;
    }
    let (host, port) = match rest.split_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().ok().filter(|p| *p != 0)?),
        None => (rest, 443),
    };
    let host_ok = !host.is_empty()
        && !host.starts_with(['.', '-'])
        && !host.ends_with(['.', '-'])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    host_ok.then(|| (host.to_ascii_lowercase(), port))
}

/// Everything the connection needs.
pub struct Params {
    /// `https://host[:port]` of the agent port.
    pub address: String,
    /// The pinned CA certificate (PEM).
    pub ca_pem: String,
    /// This node's certificate (PEM).
    pub cert_pem: String,
    /// This node's private key (PEM).
    pub key_pem: SecretString,
}

/// Builds the client TLS configuration: TLS 1.3, only the pinned CA is trusted,
/// the node certificate is presented.
///
/// # Errors
/// [`StreamError::Tls`] when a PEM block is missing or the key does not match.
pub fn client_config(params: &Params) -> Result<Arc<ClientConfig>, StreamError> {
    let tls = |what: &str, e: &dyn std::fmt::Display| StreamError::Tls(format!("{what}: {e}"));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(params.ca_pem.as_bytes()) {
        let cert = cert.map_err(|e| tls("CA certificate", &e))?;
        roots.add(cert).map_err(|e| tls("CA certificate", &e))?;
    }
    if roots.is_empty() {
        return Err(StreamError::Tls("no CA certificate found".into()));
    }
    let chain = CertificateDer::pem_slice_iter(params.cert_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| tls("node certificate", &e))?;
    let key = PrivateKeyDer::from_pem_slice(params.key_pem.expose_secret().as_bytes())
        .map_err(|e| tls("node key", &e))?;
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS13])
        .map_err(|e| tls("TLS versions", &e))?
        .with_root_certificates(roots)
        .with_client_auth_cert(chain, key)
        .map_err(|e| tls("node certificate and key", &e))?;
    Ok(Arc::new(config))
}

/// Delays between reconnect attempts: doubling from one second up to a minute,
/// each randomised between half and all of its value so that many agents
/// restarting together do not stampede the control plane.
#[derive(Debug)]
pub struct Backoff {
    current: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            current: BACKOFF_START,
        }
    }
}

impl Backoff {
    /// The next delay. `unit` is a random number in `[0, 1)`.
    pub fn next_delay(&mut self, unit: f64) -> Duration {
        let base = self.current;
        self.current = (self.current * 2).min(BACKOFF_CAP);
        base.mul_f64(0.5 + 0.5 * unit.clamp(0.0, 1.0))
    }

    /// Back to the first delay (after a connection that worked).
    pub fn reset(&mut self) {
        self.current = BACKOFF_START;
    }
}

/// A cheap random number in `[0, 1)` for jitter; not for anything secret.
fn jitter() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    f64::from(nanos % 1_000_000) / 1_000_000.0
}

enum Ended {
    Shutdown,
    /// `established` tells whether `Welcome` was received before it failed.
    Failed {
        established: bool,
        reason: String,
    },
}

/// Stays connected until `shutdown` turns `true`, reconnecting forever.
///
/// # Errors
/// Only configuration problems that retrying cannot fix.
pub async fn run(params: Params, mut shutdown: watch::Receiver<bool>) -> Result<(), StreamError> {
    let (host, port) = split_address(&params.address)
        .ok_or_else(|| StreamError::BadAddress(params.address.clone()))?;
    let name = ServerName::try_from(host.clone())
        .map_err(|_| StreamError::BadAddress(params.address.clone()))?;
    let connector = TlsConnector::from(client_config(&params)?);
    let mut backoff = Backoff::default();
    loop {
        let started = Instant::now();
        match connect_and_serve(&connector, &name, &host, port, &mut shutdown).await {
            Ended::Shutdown => return Ok(()),
            Ended::Failed {
                established,
                reason,
            } => {
                if established {
                    backoff.reset();
                }
                let delay = backoff.next_delay(jitter());
                tracing_line(&format!(
                    "connection to the control plane ended after {}s: {reason}; retrying in {}s",
                    started.elapsed().as_secs(),
                    delay.as_secs().max(1)
                ));
                tokio::select! {
                    () = tokio::time::sleep(delay) => {}
                    () = stopped(&mut shutdown) => return Ok(()),
                }
            }
        }
    }
}

/// The agent has no logging framework yet (T02.x); one line to stderr, which
/// systemd collects in the journal. Never includes keys or tokens.
#[allow(clippy::print_stderr)]
fn tracing_line(line: &str) {
    eprintln!("dhpanel-agent: {line}");
}

async fn stopped(shutdown: &mut watch::Receiver<bool>) {
    while !*shutdown.borrow_and_update() {
        if shutdown.changed().await.is_err() {
            return std::future::pending().await;
        }
    }
}

async fn connect_and_serve(
    connector: &TlsConnector,
    name: &ServerName<'static>,
    host: &str,
    port: u16,
    shutdown: &mut watch::Receiver<bool>,
) -> Ended {
    let fail = |established: bool, reason: String| Ended::Failed {
        established,
        reason,
    };
    let connecting = async {
        let tcp = TcpStream::connect((host, port))
            .await
            .map_err(|e| format!("cannot connect: {e}"))?;
        let _ = tcp.set_nodelay(true);
        let tls = connector
            .connect(name.clone(), tcp)
            .await
            .map_err(|e| format!("TLS handshake failed: {e}"))?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_FRAME_BYTES))
            .max_frame_size(Some(MAX_FRAME_BYTES));
        let url = format!("wss://{host}:{port}{STREAM_PATH}");
        let (mut ws, _) = client_async_with_config(url, tls, Some(config))
            .await
            .map_err(|e| format!("WebSocket upgrade failed: {e}"))?;
        let hello = encode(&AgentMessage::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            agent_version: crate::AGENT_VERSION.to_owned(),
        }))
        .map_err(|e| e.to_string())?;
        ws.send(Message::Text(hello.into()))
            .await
            .map_err(|e| format!("cannot send hello: {e}"))?;
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => match decode::<ControlMessage>(text.as_str()) {
                    Ok(ControlMessage::Welcome(_)) => return Ok(ws),
                    _ => {
                        return Err(
                            "the control plane did not answer hello with welcome".to_owned()
                        );
                    }
                },
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(Message::Close(frame))) => {
                    let why = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                    return Err(format!("the control plane refused the session: {why}"));
                }
                Some(Ok(_)) => return Err("unexpected message before welcome".to_owned()),
                Some(Err(e)) => return Err(format!("connection lost before welcome: {e}")),
                None => return Err("connection closed before welcome".to_owned()),
            }
        }
    };
    let mut ws = tokio::select! {
        r = timeout(CONNECT_TIMEOUT, connecting) => match r {
            Ok(Ok(ws)) => ws,
            Ok(Err(reason)) => return fail(false, reason),
            Err(_) => return fail(false, "timed out connecting".to_owned()),
        },
        () = stopped(shutdown) => return Ended::Shutdown,
    };
    tracing_line("connected to the control plane");

    // Pings from the control plane arrive every 15 s and tungstenite answers
    // them while we read. Three missed pings means the path is dead.
    let silence = Duration::from_secs(AGENT_SILENCE_TIMEOUT_SECS);
    loop {
        let next = tokio::select! {
            () = stopped(shutdown) => {
                let _ = timeout(Duration::from_secs(2), ws.close(None)).await;
                return Ended::Shutdown;
            }
            next = timeout(silence, ws.next()) => next,
        };
        let message = match next {
            Err(_) => return fail(true, "no word from the control plane".to_owned()),
            Ok(None) => return fail(true, "connection closed".to_owned()),
            Ok(Some(Err(e))) => return fail(true, format!("connection lost: {e}")),
            Ok(Some(Ok(message))) => message,
        };
        match message {
            Message::Text(text) => {
                if let Some(reply) = answer(text.as_str())
                    && ws.send(Message::Text(reply.into())).await.is_err()
                {
                    return fail(true, "cannot send".to_owned());
                }
            }
            Message::Close(frame) => {
                let why = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                return fail(true, format!("closed by the control plane: {why}"));
            }
            Message::Binary(_) => return fail(true, "unexpected binary frame".to_owned()),
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
        }
    }
}

/// Answers a frame from the control plane. Only `Ping` is implemented here;
/// the handler framework with the idempotency cache is T02.12, and everything
/// else is answered with a clear refusal so the control plane never waits for a
/// result that will not come.
fn answer(frame: &str) -> Option<String> {
    let result = match parse_operation_frame(frame) {
        Ok(Parsed::Known(envelope)) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
            match (envelope.check_fresh(now), &envelope.op) {
                (Err(error), _) => OperationResult::failure(envelope.op_id, error),
                (Ok(()), Operation::Ping { nonce }) => {
                    OperationResult::success(envelope.op_id, Output::Pong { nonce: *nonce })
                }
                (Ok(()), _) => OperationResult::failure(
                    envelope.op_id,
                    AgentError::new(
                        ErrorCode::UnsupportedOperation,
                        "this agent build does not run this operation yet",
                    ),
                ),
            }
        }
        Ok(Parsed::Unsupported { header, op_type }) => OperationResult::failure(
            header.op_id,
            AgentError::new(
                ErrorCode::UnsupportedOperation,
                format!(
                    "unknown operation `{}`",
                    op_type.chars().take(64).collect::<String>()
                ),
            ),
        ),
        Ok(Parsed::InvalidArguments { header, message }) => OperationResult::failure(
            header.op_id,
            AgentError::new(ErrorCode::InvalidArguments, message),
        ),
        // Not an operation (for example a late Welcome): nothing to answer.
        Err(_) => return None,
    };
    encode(&AgentMessage::Result(result)).ok()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_https_addresses_are_accepted() {
        for good in [
            "https://panel.example.com",
            "https://panel.example.com:8444",
            "https://127.0.0.1:8444",
            "https://a-b.example.org:1",
        ] {
            assert!(valid_stream_url(good), "{good}");
        }
        assert_eq!(
            split_address("https://Panel.Example.com:8444"),
            Some(("panel.example.com".to_owned(), 8444))
        );
        assert_eq!(
            split_address("https://x.example"),
            Some(("x.example".to_owned(), 443))
        );
        for bad in [
            "http://panel.example.com:8444",
            "wss://panel.example.com",
            "https://",
            "https://:8444",
            "https://panel.example.com/",
            "https://panel.example.com/path",
            "https://panel.example.com:0",
            "https://panel.example.com:99999",
            "https://panel.example.com:80:80",
            "https://user@panel.example.com",
            "https://panel.example.com?x=1",
            "https://pa nel.example.com",
            "https://panel.example.com\n",
            "https://-bad.example.com",
            "https://bad.example.com.",
            "https://[::1]:8444",
        ] {
            assert!(!valid_stream_url(bad), "{bad:?}");
        }
    }

    #[test]
    fn backoff_doubles_to_a_ceiling_and_jitters_within_bounds() {
        let mut b = Backoff::default();
        let secs: Vec<f64> = (0..9).map(|_| b.next_delay(1.0).as_secs_f64()).collect();
        assert_eq!(secs, [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 60.0, 60.0, 60.0]);
        b.reset();
        assert_eq!(
            b.next_delay(0.0),
            Duration::from_millis(500),
            "half of the first delay at minimum"
        );
        let mut b = Backoff::default();
        for _ in 0..20 {
            let d = b.next_delay(0.37);
            assert!(d <= BACKOFF_CAP && d >= Duration::from_millis(500));
        }
    }

    fn frame(op_id: uuid::Uuid, issued_at: i64, expires_at: i64, op: &str) -> String {
        format!(
            r#"{{"type":"operation","op_id":"{op_id}","idempotency_key":"k","issued_at":{issued_at},"expires_at":{expires_at},"protocol_version":1,"op":{op}}}"#
        )
    }

    fn result_of(frame: &str) -> OperationResult {
        match decode::<AgentMessage>(&answer(frame).expect("a reply")).unwrap() {
            AgentMessage::Result(result) => result,
            other => panic!("not a result: {other:?}"),
        }
    }

    #[test]
    fn ping_is_answered_and_everything_else_is_refused_cleanly() {
        let op_id = uuid::Uuid::now_v7();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let pong = result_of(&frame(op_id, now, now + 60, r#"{"type":"ping","nonce":7}"#));
        assert_eq!(pong.op_id, op_id);
        assert_eq!(pong.output, Some(Output::Pong { nonce: 7 }));
        assert!(pong.error.is_none());

        // Expired and not-yet-valid envelopes are refused, even for a ping.
        let late = result_of(&frame(
            op_id,
            now - 120,
            now - 60,
            r#"{"type":"ping","nonce":1}"#,
        ));
        assert_eq!(late.error.unwrap().code, ErrorCode::Expired);
        let early = result_of(&frame(
            op_id,
            now + 3600,
            now + 3660,
            r#"{"type":"ping","nonce":1}"#,
        ));
        assert_eq!(early.error.unwrap().code, ErrorCode::Expired);

        // Known but not implemented yet, and unknown operations.
        let caps = result_of(&frame(
            op_id,
            now,
            now + 60,
            r#"{"type":"report_capabilities"}"#,
        ));
        assert_eq!(caps.error.unwrap().code, ErrorCode::UnsupportedOperation);
        let unknown = result_of(&frame(op_id, now, now + 60, r#"{"type":"format_disk"}"#));
        assert_eq!(unknown.op_id, op_id);
        assert!(unknown.error.unwrap().message.contains("format_disk"));

        assert!(answer("not json").is_none());
        assert!(answer(r#"{"type":"welcome","protocol_version":1,"server_time":1}"#).is_none());
    }

    #[test]
    fn a_missing_ca_or_certificate_is_a_clear_error() {
        let params = Params {
            address: "https://x.example".into(),
            ca_pem: String::new(),
            cert_pem: String::new(),
            key_pem: SecretString::from(String::new()),
        };
        assert!(matches!(client_config(&params), Err(StreamError::Tls(m)) if m.contains("CA")));
    }
}
