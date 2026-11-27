# Android transfer boundary, ABI 1

Status: ABI 1 contract, with a P03 lifecycle implementation that has not been
compiled or exercised through a JVM/device. Kotlin never parses the TLS wire
protocol. Portable wire limits/types stay in the Rust core.

## P03 implemented subset

All eight entry points are written. Engines have a bounded opaque handle registry
(at most four engines), version checks, close/double-close handling, and a single
bounded poller. `startReceiveSession` currently creates only a local session;
its SessionStarted data is `{"transport_ready":false}`. It opens no socket,
advertises no service, and must not make Receive appear ready. `closeSession`
invalidates that session and emits SessionClosed with the closed generation and
the same `transport_ready:false` field. Other transfer/pairing commands return
InvalidState; no fake invitation, pairing, receipt, or completed file is emitted.

Current short lifecycle commands are serialized under engine state and enqueue
events immediately; there is no background command worker/runtime yet. The
32-command/progress/provider-worker limits below constrain future implementations.
P03 reserves room for the matching close event before accepting a local session.
The two FD entry points validate arguments then return InvalidState without
inspecting, duplicating, or taking ownership of a caller FD. Actual dup/I/O and
associated failure tests remain gated on an approved transfer implementation.

Local event IDs are nonzero monotonic engine-local counters encoded as 32 hex
digits, not authentication capabilities. Session IDs come from OS randomness.
Polling removes events only after JNI array creation succeeds with no pending
JVM exception. Kotlin keeps up to 64 pending events in memory until its single
coordinator calls acknowledgeThrough; UI collection starting/stopping does not
remove events. Explicit engine close abandons the engine and stops its IO pump.
No engine is created or library loaded from existing screens at this stage.

Native unit tests and Kotlin loader tests are written but unexecuted. Packaging,
real JNI calls, exception/FD paths, and APK page-size behavior are unverified.
Build entry and limits are documented in `native-transfer-build.md`.

## Entry points

Native library basename: `nuvyro_transfer_jni`. Kotlin bridge owner:
`com.ditzzy.nuvyro.core.transfer.NativeTransferBridge`, with JVM-static methods.
The initial ABI version is integer **1**, independent of package/wire versions.
Do not load native code from a Composable or build it while opening a screen.

| JVM-static method | Result / behavior |
| --- | --- |
| nativeAbiVersion(): int | 1, without allocating an engine |
| nativeEngineVersion(): String | Package version only |
| nativeCreateEngine(expectedAbi: int): long | Positive opaque handle; reject mismatch before creation |
| nativeDestroyEngine(engine: long): int | Invalidate handle/generation, request cleanup, release ownership |
| nativeSubmit(engine: long, command: byte[]): int | Validate and enqueue bounded UTF-8 JSON command |
| nativePoll(engine: long, maxEvents: int, timeoutMs: int): byte[] | Bounded UTF-8 JSON event array, empty `[]` on timeout |
| nativeAttachSourceFd(engine: long, generation: long, transferId: byte[], fileId: byte[], fd: int): int | Duplicate borrowed FD synchronously, then enqueue |
| nativeAttachDestinationFd(engine: long, generation: long, transferId: byte[], fileId: byte[], fd: int): int | Same ownership contract, private staging destination |

ID byte arrays must be exactly 16 nonzero bytes. JNI handle/generation use a
positive signed 64-bit value, never a pointer or truncated Int. Generation
increments on new session and invalidates queued/stale work at stop/destroy.
Unknown/closed handles and double destroy return InvalidHandle, not a crash.
Shutdown cannot claim a blocked Android provider immediately stopped; UI never
waits on such I/O and stale completions cannot authorize finalization.

Status codes: 0 Accepted; 1 InvalidArgument; 2 InvalidHandle; 3 AbiMismatch;
4 InvalidState; 5 QueueFull; 6 ResourceLimit; 7 InternalFailure.
Accepted means queued, not completed. Async results are events. Create/poll
failures throw `NativeTransferException(code: int)` with a fixed nonsensitive
message; zero is never a usable returned engine handle. JNI allocation failures
must preserve a pending JVM exception and clean native ownership.

## Bounded local envelopes

Command/event JSON is a local bridge format, **not** TLS wire serialization.
Do not duplicate the Rust wire parser or expose raw wire injection in Kotlin.
Each command has exactly these keys:

```json
{"abi":1,"kind":"closeSession","request_id":"11111111111111111111111111111111","generation":"1","session_id":"22222222222222222222222222222222","transfer_id":"00000000000000000000000000000000","data":{}}
```

All `u64` quantities are canonical decimal strings (no signs, leading zeros, or
floating point), constrained to signed Long on Android. IDs are 32 lowercase hex
characters; session/transfer may be zero only when that command has no such
object yet. Reject duplicate/unknown keys, unexpected data fields, non-UTF-8,
oversize, null required values, nesting beyond six containers, or unknown kinds.
Commands/events and their individual text fields obey the wire metadata limits.

| Kind | Required data / prerequisite |
| --- | --- |
| startReceiveSession | Empty; no active session; creates generation/session ID |
| createInvitation | Empty; current receiving session; returns sensitive QR event |
| connectAndPair | qr_payload (bounded pairing text), network_handle (opaque decimal u64 string), endpoints (at most two) |
| offerTransfer | files (1–128) with file_id, name, mime, size; paired/current transfer ID |
| approveOffer / declineOffer | manifest_digest (64 lowercase hex); decline also reason (wire u16 enum) |
| confirmDestinationCommit | file_id, size, digest, receipt_id, actual_name; only after Android readback/journal succeeds |
| cancelTransfer | reason (wire Cancel enum); current transfer |
| closeSession | Empty; current generation |

An endpoint contains `family` (4 or 6), `address` (8 or 32 lowercase hex digits),
`scope_id` (decimal string), and `port` (47739). IPv4/global IPv6 scope is zero;
link-local IPv6 scope must identify the selected sender-side Network/interface.
Endpoints are freshly resolved/filtered by Android, never copied blindly from a
peer URL. Network handle is the full opaque 64-bit NDK value represented by its
unsigned decimal bits, even if the JVM Long's signed representation differs;
do not reject a valid handle only because bit 63 is set or coerce it to Int.
No content URI, local path, Java Network object, FD integer, or whole file bytes
is accepted inside a JSON command. Files use the dedicated borrowed-FD calls.

Events contain `abi`, `kind`, `event_id`, `request_id`, `generation`, `session_id`,
`transfer_id`, and `data`. IDs for inapplicable objects are zero. Event kinds:
SessionStarted, InvitationCreated, Paired, OfferReceived, AwaitingSource,
AwaitingDestination, FileVerified, DestinationCommitRequired, Progress,
FileCommitted, TransferCompleted, Declined, Cancelled, DeliveryUnconfirmed,
SessionClosed, and Failed. The associated request ID is zero for unsolicited
peer-driven events. `data` uses the corresponding typed protocol metadata;
Failed includes stable code/scope; Progress includes phase/file ID and exact
decimal byte counters; InvitationCreated alone may carry the sensitive QR text.
Do not copy file URI/key/token into general progress/error events.

## Ownership and scheduling

- Commands: maximum 65,536 encoded bytes, queue capacity 32. Event batch: maximum
  65,536 bytes, at most 32 events, individual event at most 16,384 bytes. Poll
  accepts maxEvents 1–32, timeoutMs 0–1,000; exactly one consumer on dispatcher IO.
- Coalesce progress by current object and emit at most five times per second.
  Keep up to 32 pending progress objects and 16 terminal events. Never overwrite
  or drop an unconsumed terminal event. If terminal capacity is exhausted, stop
  admitting new operations until polling frees room; reject commands QueueFull.
  Close/cancel/destroy must remain available without allocating a new operation.
- Poll emits only whole events that fit; remaining events stay queued. Separate
  engine/session/transfer terminal ownership ensures bounded terminal production
  even when the UI stops polling. A session has at most one active transfer.
- Use one controlled native runtime per engine, at most two provider-I/O workers,
  four queued data chunks, no per-file runtime, no per-chunk JNI callback.
- Kotlin opens URI/staging FDs. During the synchronous call native validates
  state/limits, duplicates the borrowed FD, and owns only that duplicate through
  RAII. On failed dup/queue admission close any duplicate, return an error, and
  leave Kotlin's descriptor owned by Kotlin. Each side closes its copy once.
- dup shares file offset: never read/write the same source concurrently in Kotlin
  and Rust without a separate contract. Support non-seekable input or reject it
  explicitly when a capability needs seek; no mmap/seek/fsync assumption.
- Export/readback/AtomicFile receipt remain Android operations. Native emits
  FileCommitted only after a matching confirmDestinationCommit for the current
  session/transfer/file/hash/size; reject stale or duplicate confirmation.
- Never share JNIEnv across threads. JNI boundary catches unwindable panics,
  maps a stable error, and cleans ownership; process abort/allocation failure is
  not promised recoverable by catch_unwind. Do not globally select panic=abort as
  an undocumented substitute for an error boundary. Release Java global refs.

P03 must exercise ABI mismatch, invalid/closed handles, double destroy, queue
saturation, pending JVM exceptions, FD dup failure, cancellation/stale events,
and missing library behavior when compilation/tests are explicitly allowed.
