//! Wire types shared by the control plane and the node agent.
//!
//! See `docs/adr/0004-node-agent-protocol.md`. In short: one WebSocket per
//! node over mutual TLS, one JSON object per text frame, at most
//! [`MAX_FRAME_BYTES`]. Both ends are Rust and use these very types, so a
//! mismatch is a compile error, not a runtime surprise.
//!
//! ## Compatibility rules
//! * Changes within one [`PROTOCOL_VERSION`] are **additive**: new operations,
//!   new outputs, new optional fields (give them `#[serde(default)]`).
//!   Unknown JSON fields are ignored, never rejected.
//! * An operation an older agent does not know does not break the connection:
//!   [`parse_operation_frame`] returns [`Parsed::Unsupported`] and the agent
//!   answers that `op_id` with an `unsupported_operation` error.
//! * A breaking change bumps [`PROTOCOL_VERSION`]; the control plane keeps
//!   accepting the previous one ([`MIN_SUPPORTED_VERSION`]).
//!
//! No operation here carries a secret; if one ever must, its type gets a
//! redacting `Debug` and is left out of logs.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Version spoken by this build.
pub const PROTOCOL_VERSION: u16 = 1;
/// Oldest version this build still understands.
pub const MIN_SUPPORTED_VERSION: u16 = 1;
/// Largest frame accepted or produced, in bytes. Bulk data uses other channels.
pub const MAX_FRAME_BYTES: usize = 1 << 20;
/// Longest time an envelope may stay valid, in seconds. Bounds the replay window.
pub const MAX_ENVELOPE_LIFETIME_SECS: i64 = 3600;
/// How far in the future `issued_at` may be before the agent distrusts it.
pub const MAX_CLOCK_SKEW_SECS: i64 = 300;

/// How often the control plane pings an agent, in seconds. The agent answers
/// with a pong; a ping also tells the agent the control plane is alive.
pub const HEARTBEAT_INTERVAL_SECS: u64 = 15;
/// A session that has sent nothing (no pong, no message) for this long (three
/// missed pings) is dead: the control plane closes it and the node shows as
/// offline. The check runs on each ping tick, so a node that vanishes without a
/// goodbye is offline within this plus one interval: at most 60 seconds.
pub const LIVENESS_TIMEOUT_SECS: u64 = 45;
/// An agent that has heard nothing from the control plane for this long
/// (three missed pings) drops the connection and reconnects.
pub const AGENT_SILENCE_TIMEOUT_SECS: u64 = 45;
/// Path of the agent port's WebSocket endpoint (the only one it serves).
pub const STREAM_PATH: &str = "/agent/v1/stream";

/// The protocol version to speak with a peer announcing `peer`, or `None`
/// when the peer is too old.
#[must_use]
pub fn negotiate_version(peer: u16) -> Option<u16> {
    (peer >= MIN_SUPPORTED_VERSION).then_some(peer.min(PROTOCOL_VERSION))
}

// ------------------------------------------------------------------ errors

/// Machine-readable failure reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The agent does not know this operation (older agent, newer control plane).
    UnsupportedOperation,
    /// The operation is known but its arguments are invalid.
    InvalidArguments,
    /// The envelope's validity window has passed (or is not plausible).
    Expired,
    /// The envelope's protocol version is not supported.
    UnsupportedVersion,
    /// The agent is running as many operations as it allows; try again shortly.
    Busy,
    /// The operation did not finish in the time it had.
    Timeout,
    /// Something went wrong inside the agent; see the message.
    Internal,
}

/// A failure reported by the agent. The message is safe to show to administrators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct AgentError {
    /// What kind of failure.
    pub code: ErrorCode,
    /// Human-readable detail (English; never contains secrets).
    pub message: String,
    /// Whether trying the same operation again may succeed.
    #[serde(default)]
    pub retryable: bool,
}

impl AgentError {
    /// A non-retryable error.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
        }
    }

    /// Marks the error as worth retrying.
    #[must_use]
    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }
}

// -------------------------------------------------------------- operations

/// What the control plane asks a node to do. Every variant is a narrow,
/// typed action; there is no variant that runs a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    /// Liveness check; the agent answers [`Output::Pong`] with the same nonce.
    Ping {
        /// Echoed back.
        nonce: u64,
    },
    /// Ask the node to describe itself; answered with [`Output::Capabilities`].
    ReportCapabilities,
}

impl Operation {
    /// Wire names of every operation this build knows, in declaration order.
    pub const TYPE_NAMES: &'static [&'static str] = &["ping", "report_capabilities"];

    /// The operation's wire name.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Ping { .. } => "ping",
            Self::ReportCapabilities => "report_capabilities",
        }
    }
}

/// One operation with the metadata needed to run it exactly once, in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationEnvelope {
    /// The operations-ledger id; results refer to it.
    pub op_id: Uuid,
    /// Same key, same effect: lets the control plane retry safely.
    pub idempotency_key: String,
    /// The tenant the operation is about, when it has one.
    #[serde(default)]
    pub tenant_id: Option<Uuid>,
    /// When the control plane created it (Unix seconds).
    pub issued_at: i64,
    /// After this (Unix seconds) the agent must refuse to run it.
    pub expires_at: i64,
    /// Protocol version of this envelope.
    pub protocol_version: u16,
    /// What to do.
    pub op: Operation,
}

impl OperationEnvelope {
    /// Refuses envelopes that are expired, issued implausibly far in the
    /// future, valid for too long, or of an unsupported version. `now` is Unix
    /// seconds on the agent's clock.
    ///
    /// # Errors
    /// [`ErrorCode::Expired`] or [`ErrorCode::UnsupportedVersion`].
    pub fn check_fresh(&self, now: i64) -> Result<(), AgentError> {
        check_header(self.protocol_version, self.issued_at, self.expires_at, now)
    }
}

fn check_header(version: u16, issued_at: i64, expires_at: i64, now: i64) -> Result<(), AgentError> {
    if !(MIN_SUPPORTED_VERSION..=PROTOCOL_VERSION).contains(&version) {
        return Err(AgentError::new(
            ErrorCode::UnsupportedVersion,
            format!("protocol version {version} is not supported"),
        ));
    }
    if now >= expires_at {
        return Err(AgentError::new(
            ErrorCode::Expired,
            "the operation has expired",
        ));
    }
    if issued_at > now.saturating_add(MAX_CLOCK_SKEW_SECS) {
        return Err(AgentError::new(
            ErrorCode::Expired,
            "the operation was issued in the future; check the clocks",
        ));
    }
    if expires_at.saturating_sub(issued_at) > MAX_ENVELOPE_LIFETIME_SECS {
        return Err(AgentError::new(
            ErrorCode::Expired,
            "the operation's validity window is too long",
        ));
    }
    Ok(())
}

// ----------------------------------------------------------------- results

/// What the node reports about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Agent build version.
    pub agent_version: String,
    /// Logical CPU cores.
    pub cpu_cores: u32,
    /// Total memory in bytes.
    pub memory_bytes: u64,
    /// Size of the filesystem holding tenant data, in bytes.
    pub disk_total_bytes: u64,
    /// Free space there, in bytes.
    pub disk_free_bytes: u64,
    /// Kernel release, e.g. `6.8.0-31-generic`.
    pub kernel: String,
    /// Docker Engine version, if reachable.
    #[serde(default)]
    pub docker_version: Option<String>,
    /// How the agent reaches Docker.
    #[serde(default)]
    pub docker_mode: Option<DockerMode>,
    /// cgroup v2 is in use.
    #[serde(default)]
    pub cgroup_v2: bool,
    /// seccomp is available.
    #[serde(default)]
    pub seccomp: bool,
    /// AppArmor is enabled.
    #[serde(default)]
    pub apparmor: bool,
    /// Unprivileged user namespaces work.
    #[serde(default)]
    pub user_namespaces: bool,
}

/// How the agent reaches Docker (ADR 0004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerMode {
    /// A rootless daemon owned by the agent user. The production mode.
    Rootless,
    /// A root daemon reached through the `docker` group: root-equivalent;
    /// development only.
    Rootful,
}

/// The data an operation produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Output {
    /// Answer to [`Operation::Ping`].
    Pong {
        /// The nonce from the ping.
        nonce: u64,
    },
    /// Answer to [`Operation::ReportCapabilities`].
    Capabilities(Capabilities),
}

/// How an operation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    /// Done.
    Succeeded,
    /// Failed for good.
    Failed,
    /// Failed, but trying again may work.
    Retryable,
}

/// The agent's answer to one operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationResult {
    /// The operation this answers.
    pub op_id: Uuid,
    /// How it ended.
    pub status: ResultStatus,
    /// What it produced, on success.
    #[serde(default)]
    pub output: Option<Output>,
    /// Why it failed, otherwise.
    #[serde(default)]
    pub error: Option<AgentError>,
}

impl OperationResult {
    /// A successful result.
    #[must_use]
    pub fn success(op_id: Uuid, output: Output) -> Self {
        Self {
            op_id,
            status: ResultStatus::Succeeded,
            output: Some(output),
            error: None,
        }
    }

    /// A failed result; the status follows [`AgentError::retryable`].
    #[must_use]
    pub fn failure(op_id: Uuid, error: AgentError) -> Self {
        Self {
            op_id,
            status: if error.retryable {
                ResultStatus::Retryable
            } else {
                ResultStatus::Failed
            },
            output: None,
            error: Some(error),
        }
    }
}

// ---------------------------------------------------------------- messages

/// First message from the agent after the TLS handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// Highest protocol version the agent speaks.
    pub protocol_version: u16,
    /// Agent build version.
    pub agent_version: String,
}

/// The control plane's answer to [`Hello`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    /// The version both sides will use (see [`negotiate_version`]).
    pub protocol_version: u16,
    /// The control plane's clock (Unix seconds), so the agent can warn about drift.
    pub server_time: i64,
}

/// Frames sent by the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    /// Opens the session.
    Hello(Hello),
    /// Answers an operation.
    Result(OperationResult),
}

/// Frames sent by the control plane. Operation frames are produced with
/// [`encode_operation`] and read with [`parse_operation_frame`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMessage {
    /// Accepts the session.
    Welcome(Welcome),
    /// Asks the agent to run an operation.
    Operation(OperationEnvelope),
}

// -------------------------------------------------------------- enrollment

/// Path of the enrollment endpoint on the control plane.
pub const ENROLL_PATH: &str = "/api/v1/public/nodes/enroll";

/// What an agent sends to enrol (plain HTTPS, before it has a certificate).
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EnrollRequest {
    /// The one-time token from the "add node" screen.
    #[cfg_attr(feature = "openapi", schema(example = "dhn_…"))]
    pub token: String,
    /// PEM certificate signing request made with a fresh ECDSA P-256 key that
    /// never leaves the node.
    pub csr_pem: String,
    /// Host name of the server.
    #[serde(default)]
    pub hostname: Option<String>,
    /// Agent build version.
    #[serde(default)]
    pub agent_version: Option<String>,
}

impl std::fmt::Debug for EnrollRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the token or the request body.
        f.debug_struct("EnrollRequest")
            .field("hostname", &self.hostname)
            .field("agent_version", &self.agent_version)
            .finish_non_exhaustive()
    }
}

/// The node's new identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EnrollResponse {
    /// The node's id.
    pub node_id: Uuid,
    /// The node's name.
    pub name: String,
    /// The node certificate (PEM): client authentication for the mTLS connection.
    pub certificate_pem: String,
    /// The CA certificate (PEM) that signed it and that signs the control
    /// plane's server certificate.
    pub ca_certificate_pem: String,
    /// SHA-256 fingerprint of the CA certificate (lowercase hex), to compare
    /// with the one in the enrol command.
    pub ca_fingerprint_sha256: String,
    /// Certificate validity start (RFC 3339, UTC).
    #[cfg_attr(feature = "openapi", schema(format = DateTime))]
    pub not_before: String,
    /// Certificate expiry (RFC 3339, UTC). The agent renews before this.
    #[cfg_attr(feature = "openapi", schema(format = DateTime))]
    pub not_after: String,
    /// Protocol version the control plane speaks.
    pub protocol_version: u16,
    /// Where the agent opens its long-lived mutual-TLS connection
    /// (`https://host:port`), when the control plane has the agent port switched on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_url: Option<String>,
}

// ----------------------------------------------------------------- framing

/// Why a frame could not be produced or read.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// The frame is larger than [`MAX_FRAME_BYTES`].
    #[error("frame of {0} bytes exceeds the {MAX_FRAME_BYTES} byte limit")]
    TooLarge(usize),
    /// Not valid JSON, or not a message of the expected shape.
    #[error("malformed frame: {0}")]
    Malformed(String),
}

/// Serialises a message as one frame.
///
/// # Errors
/// [`FrameError::TooLarge`] or [`FrameError::Malformed`].
pub fn encode<T: Serialize>(message: &T) -> Result<String, FrameError> {
    let text = serde_json::to_string(message).map_err(|e| FrameError::Malformed(e.to_string()))?;
    if text.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(text.len()));
    }
    Ok(text)
}

/// Reads one frame. The size is checked before any parsing.
///
/// # Errors
/// [`FrameError::TooLarge`] or [`FrameError::Malformed`].
pub fn decode<T: for<'de> Deserialize<'de>>(frame: &str) -> Result<T, FrameError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(frame.len()));
    }
    serde_json::from_str(frame).map_err(|e| FrameError::Malformed(e.to_string()))
}

/// Frame for [`ControlMessage::Operation`].
///
/// # Errors
/// Same as [`encode`].
pub fn encode_operation(envelope: &OperationEnvelope) -> Result<String, FrameError> {
    #[derive(Serialize)]
    #[serde(tag = "type", rename = "operation")]
    struct Frame<'a> {
        #[serde(flatten)]
        envelope: &'a OperationEnvelope,
    }
    encode(&Frame { envelope })
}

/// The part of an operation frame every version understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeHeader {
    /// The operations-ledger id.
    pub op_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: String,
    /// Tenant the operation is about.
    pub tenant_id: Option<Uuid>,
    /// Creation time (Unix seconds).
    pub issued_at: i64,
    /// Expiry (Unix seconds).
    pub expires_at: i64,
    /// Protocol version.
    pub protocol_version: u16,
}

impl EnvelopeHeader {
    /// Same checks as [`OperationEnvelope::check_fresh`].
    ///
    /// # Errors
    /// [`ErrorCode::Expired`] or [`ErrorCode::UnsupportedVersion`].
    pub fn check_fresh(&self, now: i64) -> Result<(), AgentError> {
        check_header(self.protocol_version, self.issued_at, self.expires_at, now)
    }
}

/// The result of reading an operation frame on the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// A known, well-formed operation.
    Known(OperationEnvelope),
    /// A well-formed envelope carrying an operation this agent does not know.
    /// Answer it with [`ErrorCode::UnsupportedOperation`].
    Unsupported {
        /// The envelope header, to answer the right `op_id`.
        header: EnvelopeHeader,
        /// The unknown operation's wire name.
        op_type: String,
    },
    /// A known operation with invalid arguments. Answer it with
    /// [`ErrorCode::InvalidArguments`].
    InvalidArguments {
        /// The envelope header, to answer the right `op_id`.
        header: EnvelopeHeader,
        /// What is wrong.
        message: String,
    },
}

/// Reads an operation frame without letting an unknown operation kill the
/// session: the header is parsed first, the operation second.
///
/// # Errors
/// [`FrameError`] when the frame is not an operation envelope at all (no
/// usable header), in which case there is no `op_id` to answer.
pub fn parse_operation_frame(frame: &str) -> Result<Parsed, FrameError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(frame.len()));
    }
    #[derive(Deserialize)]
    struct Raw {
        #[serde(rename = "type")]
        kind: String,
        op_id: Uuid,
        idempotency_key: String,
        #[serde(default)]
        tenant_id: Option<Uuid>,
        issued_at: i64,
        expires_at: i64,
        protocol_version: u16,
        op: serde_json::Value,
    }
    let raw: Raw = serde_json::from_str(frame).map_err(|e| FrameError::Malformed(e.to_string()))?;
    if raw.kind != "operation" {
        return Err(FrameError::Malformed(format!(
            "expected an operation frame, got `{}`",
            raw.kind
        )));
    }
    let header = EnvelopeHeader {
        op_id: raw.op_id,
        idempotency_key: raw.idempotency_key,
        tenant_id: raw.tenant_id,
        issued_at: raw.issued_at,
        expires_at: raw.expires_at,
        protocol_version: raw.protocol_version,
    };
    let op_type = raw
        .op
        .get("type")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| FrameError::Malformed("operation has no `type`".to_owned()))?;
    match serde_json::from_value::<Operation>(raw.op) {
        Ok(op) => Ok(Parsed::Known(OperationEnvelope {
            op_id: header.op_id,
            idempotency_key: header.idempotency_key,
            tenant_id: header.tenant_id,
            issued_at: header.issued_at,
            expires_at: header.expires_at,
            protocol_version: header.protocol_version,
            op,
        })),
        Err(_) if !Operation::TYPE_NAMES.contains(&op_type.as_str()) => {
            Ok(Parsed::Unsupported { header, op_type })
        }
        Err(e) => Ok(Parsed::InvalidArguments {
            header,
            message: e.to_string(),
        }),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use serde_json::json;

    use super::*;

    const ID: &str = "0190b6a2-0000-7000-8000-000000000001";

    fn id() -> Uuid {
        ID.parse().unwrap()
    }

    fn envelope(op: Operation) -> OperationEnvelope {
        OperationEnvelope {
            op_id: id(),
            idempotency_key: "tenant-1:create".into(),
            tenant_id: Some("0190b6a2-0000-7000-8000-0000000000aa".parse().unwrap()),
            issued_at: 1_000,
            expires_at: 1_300,
            protocol_version: PROTOCOL_VERSION,
            op,
        }
    }

    fn sample_capabilities() -> Capabilities {
        Capabilities {
            agent_version: "0.1.0".into(),
            cpu_cores: 4,
            memory_bytes: 8 << 30,
            disk_total_bytes: 80 << 30,
            disk_free_bytes: 40 << 30,
            kernel: "6.8.0-31-generic".into(),
            docker_version: Some("29.9.3".into()),
            docker_mode: Some(DockerMode::Rootless),
            cgroup_v2: true,
            seccomp: true,
            apparmor: true,
            user_namespaces: true,
        }
    }

    #[test]
    fn every_message_survives_a_round_trip() {
        for op in [Operation::Ping { nonce: 7 }, Operation::ReportCapabilities] {
            let e = envelope(op);
            let frame = encode_operation(&e).unwrap();
            assert_eq!(
                parse_operation_frame(&frame).unwrap(),
                Parsed::Known(e.clone())
            );
            // The generic path agrees with the dedicated one.
            let generic: ControlMessage = decode(&frame).unwrap();
            assert_eq!(generic, ControlMessage::Operation(e));
        }
        let results = [
            OperationResult::success(id(), Output::Pong { nonce: 7 }),
            OperationResult::success(id(), Output::Capabilities(sample_capabilities())),
            OperationResult::failure(id(), AgentError::new(ErrorCode::Internal, "boom")),
            OperationResult::failure(
                id(),
                AgentError::new(ErrorCode::Internal, "try later").retryable(),
            ),
        ];
        for r in results {
            let msg = AgentMessage::Result(r);
            assert_eq!(decode::<AgentMessage>(&encode(&msg).unwrap()).unwrap(), msg);
        }
        let hello = AgentMessage::Hello(Hello {
            protocol_version: 1,
            agent_version: "0.1.0".into(),
        });
        assert_eq!(
            decode::<AgentMessage>(&encode(&hello).unwrap()).unwrap(),
            hello
        );
        let welcome = ControlMessage::Welcome(Welcome {
            protocol_version: 1,
            server_time: 1_234,
        });
        assert_eq!(
            decode::<ControlMessage>(&encode(&welcome).unwrap()).unwrap(),
            welcome
        );
    }

    #[test]
    fn the_wire_format_is_pinned() {
        // These strings are the contract with deployed agents: change them only
        // together with PROTOCOL_VERSION or as a purely additive extension.
        let frame = encode_operation(&envelope(Operation::Ping { nonce: 7 })).unwrap();
        assert_eq!(
            frame,
            format!(
                r#"{{"type":"operation","op_id":"{ID}","idempotency_key":"tenant-1:create","tenant_id":"0190b6a2-0000-7000-8000-0000000000aa","issued_at":1000,"expires_at":1300,"protocol_version":1,"op":{{"type":"ping","nonce":7}}}}"#
            )
        );
        assert_eq!(
            encode(&AgentMessage::Hello(Hello {
                protocol_version: 1,
                agent_version: "0.1.0".into()
            }))
            .unwrap(),
            r#"{"type":"hello","protocol_version":1,"agent_version":"0.1.0"}"#
        );
        assert_eq!(
            encode(&AgentMessage::Result(OperationResult::success(
                id(),
                Output::Pong { nonce: 7 }
            )))
            .unwrap(),
            format!(
                r#"{{"type":"result","op_id":"{ID}","status":"succeeded","output":{{"type":"pong","nonce":7}},"error":null}}"#
            )
        );
        assert_eq!(
            encode(&AgentMessage::Result(OperationResult::failure(
                id(),
                AgentError::new(ErrorCode::UnsupportedOperation, "no such operation")
            )))
            .unwrap(),
            format!(
                r#"{{"type":"result","op_id":"{ID}","status":"failed","output":null,"error":{{"code":"unsupported_operation","message":"no such operation","retryable":false}}}}"#
            )
        );
    }

    #[test]
    fn the_enrollment_wire_format_is_pinned() {
        let request = EnrollRequest {
            token: "dhn_x".into(),
            csr_pem: "csr".into(),
            hostname: Some("h".into()),
            agent_version: None,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"token":"dhn_x","csr_pem":"csr","hostname":"h","agent_version":null}"#
        );
        // Older agents may omit the optional fields.
        let minimal: EnrollRequest =
            serde_json::from_str(r#"{"token":"t","csr_pem":"c"}"#).unwrap();
        assert_eq!((minimal.hostname, minimal.agent_version), (None, None));
        let dbg = format!("{request:?}");
        assert!(
            dbg.contains("hostname") && !dbg.contains("dhn_x") && !dbg.contains("csr"),
            "{dbg}"
        );

        let response = EnrollResponse {
            node_id: id(),
            name: "n1".into(),
            certificate_pem: "cert".into(),
            ca_certificate_pem: "ca".into(),
            ca_fingerprint_sha256: "ab".into(),
            not_before: "2026-01-01T00:00:00Z".into(),
            not_after: "2026-04-01T00:00:00Z".into(),
            protocol_version: 1,
            stream_url: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            json,
            format!(
                r#"{{"node_id":"{ID}","name":"n1","certificate_pem":"cert","ca_certificate_pem":"ca","ca_fingerprint_sha256":"ab","not_before":"2026-01-01T00:00:00Z","not_after":"2026-04-01T00:00:00Z","protocol_version":1}}"#
            )
        );
        // An older control plane omits the field; it reads back as absent.
        let with_url = EnrollResponse {
            stream_url: Some("https://panel.example.com:8444".into()),
            ..response.clone()
        };
        let json_with_url = serde_json::to_string(&with_url).unwrap();
        assert!(json_with_url.ends_with(r#""stream_url":"https://panel.example.com:8444"}"#));
        assert_eq!(
            serde_json::from_str::<EnrollResponse>(&json_with_url).unwrap(),
            with_url
        );
        assert_eq!(
            serde_json::from_str::<EnrollResponse>(&json).unwrap(),
            response
        );
        // A newer control plane may add fields.
        let newer = json.replacen('{', r#"{"future":true,"#, 1);
        assert_eq!(
            serde_json::from_str::<EnrollResponse>(&newer).unwrap(),
            response
        );
        assert_eq!(ENROLL_PATH, "/api/v1/public/nodes/enroll");
    }

    #[test]
    fn unknown_operations_do_not_break_the_session() {
        let frame = json!({
            "type": "operation", "op_id": ID, "idempotency_key": "k",
            "issued_at": 1000, "expires_at": 1300, "protocol_version": 1,
            "op": {"type": "provision_tenant_stack", "tenant": "x", "plan": {"cpus": 2}}
        })
        .to_string();
        match parse_operation_frame(&frame).unwrap() {
            Parsed::Unsupported { header, op_type } => {
                assert_eq!(op_type, "provision_tenant_stack");
                assert_eq!(header.op_id, id());
                assert_eq!(header.tenant_id, None);
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn known_operations_with_bad_arguments_are_reported_as_such() {
        let frame = json!({
            "type": "operation", "op_id": ID, "idempotency_key": "k",
            "issued_at": 1000, "expires_at": 1300, "protocol_version": 1,
            "op": {"type": "ping", "nonce": "not a number"}
        })
        .to_string();
        match parse_operation_frame(&frame).unwrap() {
            Parsed::InvalidArguments { header, message } => {
                assert_eq!(header.op_id, id());
                assert!(
                    message.contains("nonce") || message.contains("invalid type"),
                    "{message}"
                );
            }
            other => panic!("expected InvalidArguments, got {other:?}"),
        }
    }

    #[test]
    fn newer_peers_may_add_fields_everywhere() {
        // Extra fields on envelope, operation, hello and capabilities are ignored.
        let frame = json!({
            "type": "operation", "op_id": ID, "idempotency_key": "k", "future_field": [1, 2],
            "issued_at": 1000, "expires_at": 1300, "protocol_version": 1,
            "op": {"type": "ping", "nonce": 1, "trace": "abc"}
        })
        .to_string();
        assert!(matches!(
            parse_operation_frame(&frame).unwrap(),
            Parsed::Known(_)
        ));
        let hello =
            r#"{"type":"hello","protocol_version":1,"agent_version":"9.9","features":["x"]}"#;
        assert!(decode::<AgentMessage>(hello).is_ok());
        // Capabilities written by an older agent (without the optional fields) still parse.
        let old = r#"{"agent_version":"0.0.1","cpu_cores":2,"memory_bytes":1,"disk_total_bytes":2,"disk_free_bytes":1,"kernel":"6.8"}"#;
        let c: Capabilities = serde_json::from_str(old).unwrap();
        assert_eq!(
            (c.docker_version, c.docker_mode, c.cgroup_v2),
            (None, None, false)
        );
    }

    #[test]
    fn frames_without_a_usable_header_are_errors_not_unsupported() {
        for bad in [
            "not json",
            "{}",
            r#"{"type":"welcome","protocol_version":1,"server_time":1}"#,
            &json!({"type":"operation","op_id":"nope","idempotency_key":"k","issued_at":1,"expires_at":2,"protocol_version":1,"op":{"type":"ping","nonce":1}}).to_string(),
            &json!({"type":"operation","op_id":ID,"idempotency_key":"k","issued_at":1,"expires_at":2,"protocol_version":1,"op":{"nonce":1}}).to_string(),
        ] {
            assert!(matches!(parse_operation_frame(bad), Err(FrameError::Malformed(_))), "{bad}");
        }
    }

    #[test]
    fn oversized_frames_are_refused_before_parsing() {
        let big = "x".repeat(MAX_FRAME_BYTES + 1);
        assert!(matches!(
            decode::<AgentMessage>(&big),
            Err(FrameError::TooLarge(_))
        ));
        assert!(matches!(
            parse_operation_frame(&big),
            Err(FrameError::TooLarge(_))
        ));
        let hello = AgentMessage::Hello(Hello {
            protocol_version: 1,
            agent_version: "y".repeat(MAX_FRAME_BYTES),
        });
        assert!(matches!(encode(&hello), Err(FrameError::TooLarge(_))));
    }

    #[test]
    fn envelopes_are_accepted_only_inside_their_validity_window() {
        let e = envelope(Operation::ReportCapabilities); // valid 1000..1300
        assert!(e.check_fresh(1_000).is_ok());
        assert!(e.check_fresh(1_299).is_ok());
        assert_eq!(e.check_fresh(1_300).unwrap_err().code, ErrorCode::Expired);
        assert_eq!(e.check_fresh(9_999).unwrap_err().code, ErrorCode::Expired);
        // Issued far in the future (agent clock is behind, or the envelope is forged).
        assert_eq!(
            e.check_fresh(1_000 - MAX_CLOCK_SKEW_SECS - 1)
                .unwrap_err()
                .code,
            ErrorCode::Expired
        );
        assert!(e.check_fresh(1_000 - MAX_CLOCK_SKEW_SECS).is_ok());
        // A window longer than allowed is a replay risk.
        let mut long = e.clone();
        long.expires_at = long.issued_at + MAX_ENVELOPE_LIFETIME_SECS + 1;
        assert_eq!(
            long.check_fresh(1_000).unwrap_err().code,
            ErrorCode::Expired
        );
        // Unsupported versions.
        for v in [0, PROTOCOL_VERSION + 1, u16::MAX] {
            let mut bad = e.clone();
            bad.protocol_version = v;
            assert_eq!(
                bad.check_fresh(1_000).unwrap_err().code,
                ErrorCode::UnsupportedVersion,
                "{v}"
            );
        }
        // Extreme values do not overflow.
        let mut extreme = e;
        extreme.issued_at = i64::MIN;
        extreme.expires_at = i64::MAX;
        assert!(extreme.check_fresh(i64::MAX - 1).is_err());
    }

    #[test]
    fn versions_are_negotiated_down_never_up() {
        assert_eq!(negotiate_version(PROTOCOL_VERSION), Some(PROTOCOL_VERSION));
        assert_eq!(
            negotiate_version(PROTOCOL_VERSION + 5),
            Some(PROTOCOL_VERSION)
        );
        assert_eq!(
            negotiate_version(MIN_SUPPORTED_VERSION),
            Some(MIN_SUPPORTED_VERSION)
        );
        assert_eq!(
            negotiate_version(MIN_SUPPORTED_VERSION.wrapping_sub(1)),
            if MIN_SUPPORTED_VERSION == 0 {
                Some(PROTOCOL_VERSION)
            } else {
                None
            }
        );
    }

    #[test]
    fn operation_names_match_the_wire_and_the_list() {
        for op in [Operation::Ping { nonce: 1 }, Operation::ReportCapabilities] {
            let wire = serde_json::to_value(&op).unwrap();
            assert_eq!(wire["type"], op.type_name());
            assert!(Operation::TYPE_NAMES.contains(&op.type_name()));
        }
        assert_eq!(
            Operation::TYPE_NAMES.len(),
            2,
            "update TYPE_NAMES and type_name() together with the enum"
        );
    }

    #[test]
    fn failures_map_to_the_right_status() {
        let id = id();
        let retry =
            OperationResult::failure(id, AgentError::new(ErrorCode::Internal, "x").retryable());
        assert_eq!(retry.status, ResultStatus::Retryable);
        let fail = OperationResult::failure(id, AgentError::new(ErrorCode::InvalidArguments, "x"));
        assert_eq!(fail.status, ResultStatus::Failed);
        assert!(fail.output.is_none() && fail.error.is_some());
        let ok = OperationResult::success(id, Output::Pong { nonce: 1 });
        assert!(ok.error.is_none() && ok.output.is_some());
        assert_eq!(
            AgentError::new(ErrorCode::Expired, "late").to_string(),
            "Expired: late"
        );
    }
}
