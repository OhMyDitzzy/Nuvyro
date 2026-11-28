# Nuvyro transfer engine

This Cargo workspace shares the Nuvyro Git repository with the Android app.

## Components

- `nuvyro-transfer-core`: portable protocol definitions, validation, and limits.
- `nuvyro-transfer-jni`: Android JNI lifecycle and bounded event delivery.

Package version, wire version 1.0, and JNI ABI 1 are independent. Kotlin owns UI,
permissions, discovery, Android Network selection, and URI/SAF integration.
The profile endpoint is separate from file transfer. The local-session API
reports `transport_ready:false`; it does not advertise or authorize file access.
FD attachment requires an approved transfer/storage phase.

## Core framing and integrity

Feed TLS plaintext to `StreamDecoder::push`, then process its single returned
frame before feeding the unconsumed input suffix. Partial headers and bodies
are retained within the wire ceilings; invalid input, EOF, and cancellation
close the decoder. A clean EOF succeeds only at a frame boundary.
The transport must cap its downstream data queue at four chunks.

`Frame::decode` returns a borrowed `Message`; keep the frame alive while using
its strings or chunk bytes. `Manifest::decode` preserves the original encoding
and order and calculates the domain-prefixed BLAKE3 digest. Message and frame
debug formatting omits tokens, names, and data. Owned inbound bodies are cleared
on cancellation or drop. `encode_message` validates outgoing messages with the
same wire policy and returns an `EncodedFrame` with redacted Debug and storage
that is cleared on drop. The encoder reserves its bounded capacity before
copying tokens. `encode_manifest` preserves the offered order and strings;
consent must bind to those exact encoded bytes.

`FileStreamVerifier` validates one expected file context, byte count, sequential
offsets, and incremental BLAKE3. Its local `destination_ready` transition follows
`FileBegin`; a peer frame cannot make that transition. `IntegrityChecked` covers
received bytes only. It does not assert staging readback, destination export,
receipt persistence, or completion. The owning coordinator must enforce
authentication, generation, consent, deadlines, file order, and storage policy,
and close the connection on decoding or sequence failure.

## Pairing QR input

`PairingQr::decode` accepts borrowed scanner bytes with the exact pairing prefix,
canonical unpadded base64url, a 512-byte text ceiling, and a fixed 104-byte decode
buffer. It validates version, IDs, pin/token shape, port, and TTL hint. It performs
no network, authentication, invitation consumption, or expiry decision.

`PairingQr`, `SpkiPin`, and `InvitationToken` have redacted debug output and no
implicit cloning or serialization. Pin/token access borrows their bytes; their
owned storage and the temporary decoded buffer use zeroization on drop. Callers
must clear their own scan buffers and avoid copying secrets into logs or saved
state. This does not promise erasure of copies left by moves or other libraries.
The TTL hint must never create or extend the receiver's monotonic expiry.

## Internal TLS client and handshake

The transport-private client configuration uses an explicit ring provider and
TLS 1.3 only, the fixed SNI name `nuvyro.local`, and ALPN `nuvyro-transfer/1`.
Resumption, early data, key logging, and secret extraction are disabled. The
owning worker must reject every TLS/I/O error and check the negotiated version,
ALPN, and single peer certificate before queuing any application bytes.

The verifier requires one X.509 v3 leaf of at most 4 KiB, an uncompressed P-256
public key, ECDSA/SHA-256 self-signature, explicit CA=false, digitalSignature,
serverAuth, and the fixed SAN DNS name. It rejects duplicate, malformed, and
unhandled critical extensions, checks wall-time validity and a maximum validity
span of 86,700 seconds, and compares SHA-256 of the complete SPKI with the scanned
pin using constant-time comparison. TLS CertificateVerify is checked separately
through rustls and its selected crypto provider; no public CA roots are trusted.

Client configuration performs no socket I/O. On Android, the private handshake
driver takes ownership of the already connected, Network-bound socket and uses
nonblocking reads, writes, and readiness polls. It keeps the original connect
attempt's monotonic deadline, checks cancellation and the current native/route/
Network/session context around each operation, and drops the socket on every
failure. The returned TLS connection is opaque and exposes no application I/O.
The driver is not connected to the JNI engine yet. Even a successful TLS profile
check does not authenticate an invitation, match the secure session hello,
authorize a file, or replace receiver consent.

## Outgoing pairing operation

On the owning worker, `perform_pairing` consumes the pending Network-bound socket,
a decoded QR, a claimed name, and a callback returning the current connection
context. The QR supplies both the TLS pin and the Hello/invitation fields; its
session must match the endpoint context before any TLS write. The operation
performs pinned TLS first and only then drives Hello/Ack and authentication on
that same connection. It opens no additional socket and starts no thread.

The original TCP/TLS deadline remains in force through TLS completion. Hello and
authentication then each get the gate's absolute ten-second deadline. Only a
protocol phase transition changes a deadline. Cancellation and equality of the
native generation, route generation, Network, and secure session are checked
around I/O and readiness polls of at most 50 ms. Backward clock observations,
EOF, malformed input, TLS/socket failures, and stale context release ownership.
Interrupted calls and partial writes retry within the same phase budget.

The operation queues one control frame at a time, limits TLS output buffering
to 1 KiB, and confirms the frame to the gate only after all its TLS records are
flushed. The incoming control parser uses a fixed 41-byte buffer, reads the
six-byte header before its body, and accepts only HelloAck, PairResult, or Error.
Other message types and oversized lengths fail without allocating their bodies.
Unsolicited plaintext already buffered before a request, after an Ack, or after
PairResult is rejected. This check cannot establish future network liveness.
The returned `PairedConnection` owns the socket and exposes only its PairContext;
it grants no file I/O or transfer consent. Its owner must keep enforcing session,
route, cancellation, and subsequent protocol deadlines for any later operation.
The JNI engine and Android UI do not call this operation yet.

## Internal receiver identity and pairing control

`ReceiverIdentity` owns a fresh P-256 session key, its self-signed certificate,
and one invitation store bound to the same session ID and SPKI pin. Certificate
validity is wall time minus five minutes through creation plus 24 hours. New
server connections and invitations fail after expiry or close. The explicit
ring server configuration accepts TLS 1.3 and the fixed ALPN, with tickets,
session storage, early/half-RTT data, key logging, and secret extraction disabled.
Temporary serialized key storage is cleared on drop; crypto-library copies have
their own lifetimes. Closing/dropping the identity revokes its invitation store.
The owning worker must poll `check_live` and abort/drop all existing connections
on session expiry, generation changes, cancellation, or TLS errors.

`ReceiverAdmission` belongs to one Receive session and must survive invitation
replacement and failed connections. It admits at most two unpaired sockets and
eight new TLS attempts per rolling 60 seconds. A failed or cancelled handshake
releases its concurrency slot but does not refund its attempt. Capacity and
pre-cancelled rejections do not start TLS or consume an attempt. Closing or
dropping the admission owner invalidates its remaining permits; clock reversal
closes it permanently. Use a distinct cancellation token for each connection.

The receiver context binds native and listener-route generations to the secure
session ID. It requires no client Network handle on a hotspot host and does not
prove network provenance. The listener must first validate the accepted route
and peer policy and make the socket nonblocking. `perform_receiver_handshake`
then drives only TLS records on that pre-admitted socket, starts no listener or
thread, and enforces the original ten-second admission deadline even if the
socket waited before the driver started. Cancellation, context, admission, and
identity lifetime checks surround I/O and readiness polls of at most 50 ms.

The server must receive the client's Finished, flush pending records, and verify
TLS 1.3 and the fixed ALPN before returning `VerifiedIncoming`. That opaque owner
retains the socket, TLS state, and unpaired permit for Hello and invitation
authentication; TLS completion alone does not release a slot or authenticate
the sender. It exposes no application I/O or file capability. A later control
owner must continue lifecycle checks and enforce its own protocol deadlines;
the returned object does not monitor the socket on its own.

An invitation replaces the previous unused invitation, expires after 120
monotonic seconds, and is consumed under the same lock as constant-time ID/token
comparison. Five invalid attempts in a rolling minute invalidate it. Valid
attempts remain consumed if pair allocation or the connection later fails.
Invitation and pair IDs are checked against session-local ID history; the store
retains at most 1,024 IDs, after which a new receiver session is required.

After verified TLS, `HandshakeGate` sequences Hello/Ack and invitation/PairResult
with separate absolute ten-second Hello and authentication deadlines. The owner
must poll `check_time` even without peer traffic, send only the emitted control
frames, and call `sent` only after each complete frame and pending TLS records
have been flushed. Wrong direction, session, phase, or pre-pair Error IDs fail
the gate permanently. Abort clears retained credentials; the outgoing scan is
dropped as soon as its authentication frame is encoded. The receiver reports a
`PairContext` only after PairResult is flushed; the sender reports the matching
context after receiving it. This context contains session and pair IDs only.
Transfer IDs, manifest consent, file access, and transfer readiness belong to
subsequent operations. These private components expose no general application I/O
entry point. The receiver worker owns incoming pairing on accepted TLS sockets.

## Incoming pairing operation

`perform_receiver_pairing` consumes one pre-admitted socket and uses the same
`ReceiverIdentity` for TLS and Hello/invitation authentication. It starts no
listener or thread. Hello and authentication each have an absolute ten-second
deadline; authentication time starts only after the complete Ack and its TLS
records are flushed within the Hello budget. Retries and peer traffic never
extend a deadline. Identity, context, admission, cancellation, and clock checks
continue around reads, writes, and bounded polls.

The receiver reads the six-byte header before the body into fixed 184-byte
storage that is cleared on drop. It accepts only the expected Hello or
PairAuthenticate, or a connection Error. Wrong direction, oversized or malformed
input, and unsolicited plaintext already buffered at control boundaries close
the connection. These checks do not detect every record still in the network or
establish future liveness. Only Ack and PairResult are sent by this operation.

After invitation authentication, the receiver reserves its one paired slot
before sending PairResult. The reservation is bound to both the admission owner
and the original connection. The unpaired slot remains held until PairResult
and all TLS records are flushed, after which ownership is promoted. Failed
sending or cancellation releases both slots without restoring the consumed
invitation. A second valid authentication while a paired slot is held also
consumes its invitation and fails closed. The TLS attempt limit is unchanged.

`PairedIncoming` retains the socket, TLS state, and paired slot and exposes only
the PairContext and bounded claimed display name. Its Debug omits that name.
The name is a peer claim, not a permanent verified identity. Pairing grants no
manifest consent, file capability, or transfer readiness; later operations must
continue lifecycle and protocol-deadline checks. The native receive-session
worker invokes this operation and retains at most one paired connection.

## Internal receiver listener and routing

`open_receiver_listener` owns a session's listening sockets, identity, admission,
and cancellation. An atomic identity claim prevents a second listener owner
from creating another admission budget for the same session. `ReceiverRoutes`
contains numeric addresses assigned to one
selected interface, validated prefixes, and the receiver context. At most 16
local addresses are accepted. Every selected local address must bind on 47739;
startup fails and releases all opened sockets if any bind or verification fails.
There is no wildcard, port, address, or default-network fallback. IPv6 sockets
are v6-only. Wi-Fi listeners bind the explicit nonzero Android Network before
the local address; local-only hotspot listeners require no fabricated Network.

The Android route reader uses the selected Wi-Fi Network, or the active hotspot
reservation owner's current view of interfaces and all visible client Networks.
Hotspot snapshots exclude client interfaces and addresses, including uplinks and
VPNs, and require exactly one eligible unmanaged interface. Incomplete reads,
ambiguous interfaces, missing reservation ownership, and oversized snapshots
fail closed. These snapshots are routing policy, not proof of physical ingress
or peer identity. `AndroidReceiverSession` explicitly opens an engine and supplies
snapshots through the bounded JNI command channel. Its coordinator matches native
generation, SID and route generation, acknowledges owned events and waits for
`ReceiverListening`. Network callbacks and periodic refresh invalidate stale work.
Route drift, lost permission/reservation, cancellation or native failure stops the
session and requires a new identity; the UI does not start this owner yet.

On selected Wi-Fi, the coordinator advertises the transfer service only after
the matching native listener event, with public version/session TXT and fixed
port 47739. The registration is scoped to that Network on Android 33+, while
older NSD APIs are global routing hints. Hotspot receivers retain the gateway
path without a fabricated Network. Cancellation revokes advertisement before
native close; late callbacks only clean up their original registration. Platform
unregistration is asynchronous and does not authorize or preserve a TLS session.

Each `accept_receiver` call performs at most one readiness poll of at most 50 ms
and one nonblocking accept. The accepted socket must retain close-on-exec and
be made nonblocking. Its actual local address must match the polled listener,
its queried peer must match accept's peer, and its unicast address and IPv6 scope
must belong to the selected interface's most specific matching prefix. Self,
subnet-boundary, client-local, wrong-scope, and off-subnet peers are rejected
before TLS admission. Interrupted calls return control without a drain loop.

Accepted sockets receive a distinct cancellation token and share the receiver's
two-unpaired/eight-attempt/one-paired admission across every bound address. Time
spent validating an accepted socket remains in its original ten-second TLS
budget. `AcceptedReceiver` keeps the exact listener identity and route lease and
can only be consumed by `pair_accepted_receiver`; it exposes no raw socket, FD,
or plaintext I/O. Its pairing callback revalidates the full current routing
snapshot, not just a potentially stale generation number.

Listener close/drop, cancellation, route change, backward clock, or identity
expiry revokes the session and admission. The listener's owning worker closes
its descriptors; other workers observe revocation during their lifecycle checks.
No thread, advertisement, engine readiness event, or UI is started by construction
of the route reader. The native listener runs only when its owner explicitly
opens and polls it. Runtime Android Network/hotspot behavior still requires
platform validation.

## Contracts and build

- [Wire protocol](../../docs/transfer-protocol-v1.md)
- [Security contract](../../docs/transfer-security-v1.md)
- [Android boundary](../../docs/transfer-jni-v1.md)
- [Native build](../../docs/native-transfer-build.md)

The workspace pins Rust 1.100.0 and requires Rust 1.100 or later, edition 2024.
Use Cargo.lock for dependency resolution. Keep generated target directories and
shared libraries out of Git. Both crates have `publish = false`; no SPDX license
is declared in their manifests.

## Local checks

Run from this directory:

```sh
cargo metadata --format-version 1 --no-deps --frozen
cargo fmt --all -- --check
python3 -B tools/check_contract_vectors.py
python3 -B tools/test_android_elf.py
```

The Python commands validate fixture encodings and synthetic artifact headers;
they do not execute the Rust implementation or establish TLS/device compatibility.
