# ADR 0004 — The protocol between the control plane and the node agents

Status: accepted (implemented in `crates/node-protocol`, `crates/node-gateway`, `apps/node-agent`)

## Context
The control plane must have nodes do typed work (create a tenant, write a site's configuration,
restore a backup) and receive metrics, logs and answers. Nodes may be behind NAT. A compromised
node must not be able to act on other nodes, and the agent must never run a shell.

## Decision
- **Direction:** the agent dials the control plane; nothing dials the agent, so a node needs no
  open inbound port.
- **Transport:** a long-lived **WebSocket over mutual TLS 1.3**. Frames are JSON text (one
  envelope per frame, at most `MAX_FRAME_BYTES`). Large data (backups, SQL imports, files) moves
  in separate streamed transfers, not in frames.
- **Identity:** a small CA inside the control plane signs each node's certificate after a
  one-time enrollment token (15 minutes). Certificates are rotated before they expire and can be
  revoked; the gateway checks the certificate against the `nodes` table on every connection.
- **Operations:** a closed `Operation` enum (typed fields, no free-form command), wrapped in an
  envelope with `op_id`, an idempotency key, `issued_at` / `expires_at` and the protocol version.
  The agent checks expiry, tenant ownership, paths and image allowlist before it acts.
- **Delivery:** the operations ledger (`operations` table) keeps work until a node answers;
  at-least-once delivery with idempotent operations on the node.
- **Versioning:** `PROTOCOL_VERSION`; changes inside a version are additive. A breaking change
  raises the version; the control plane accepts the previous version for one release.

## Alternatives considered
- **gRPC (tonic)**: strongly typed with streaming and back-pressure, and a good fit. Not chosen
  because the typed layer is already our own enum (serde, round-trip tested), the extra
  code generation and a second schema language would not remove it, and a WebSocket passes
  through more proxies and is easy to inspect when debugging.
- **HTTP polling by the agent**: simple, but slow for work and costly for metrics.
- **SSH from the control plane**: needs an open port and root-like credentials on every node.

## Consequences
- Adding an operation means adding a variant, an agent handler and a test; nothing else.
- Backpressure is done by the ledger (work waits in the database), not by the socket.
