# Transfer protocol 1.0

This document specifies the wire format and required behavior. Incompatible changes require a
version change and updated fixtures. Security requirements are in
`transfer-security-v1.md`; the Android boundary is in `transfer-jni-v1.md`.

## Scope and discovery

- Wire version is **1.0**. The package version is independent. Peers must support
  exactly 1.0 initially; other major/minor versions or nonzero capabilities fail
  with `UnsupportedVersion`. There is no plaintext or older-protocol fallback.
- Use one TCP/TLS connection, one paired sender, one pending offer, and one active
  transfer per receiving engine. Files are sequential; local capability/space
  policy may limit the batch further. No resume or compression.
- TLS transfer listener uses TCP **47739**. A bind conflict fails startup; no
  hidden port fallback. This is an application choice, not a registered service.
- Advertise a separate NSD type **`_nuvyro-xfer._tcp.`**, TXT `v=1.0` and
  `sid=<32 lowercase hex digits of secure session ID>`, with port 47739. These
  public values are routing hints. Do not publish a token, QR, or private key.
- Existing `_nuvyro._tcp.`, NVP1, and hotspot profile port 47738 remain profile-only
  and byte-compatible. Profile IDs and secure session IDs are separate; a name,
  photo, or profile selection never authorizes a transfer.
- Android selects the current Wi-Fi Network and maintains a generation-scoped
  endpoint registry. Match the secure service session to the scanned QR. On a
  hotspot, try the current Wi-Fi gateway on 47739. No subnet scan, arbitrary URL,
  default-network fallback, or assumption that the AP host has a client Network.
- Bind the outgoing native socket to that Network before connect; preserve the
  sender-side IPv6 scope. Network loss invalidates candidates and pairing. Do
  not reuse an endpoint because an old avatar remains visible. QR selection is
  authoritative for the secure session; any conflicting UI selection must be
  corrected explicitly rather than silently called verified.

## Scalars and identifiers

All integers are unsigned, big-endian, with checked arithmetic. `u16`, `u32`, and
`u64` mean 2, 4, and 8 bytes. An ID is 16 nonzero bytes, generated with an OS
CSPRNG and checked for collision within its owning engine/session. Session,
invitation, pair, transfer, file, and receipt IDs have distinct roles. They are
not bearer authentication. All-zero IDs are allowed only where specified.

`text` is `u16 byte_length || UTF-8 bytes`: strict UTF-8, no BOM stripping or
normalization on the wire. A decoder rejects invalid lengths, trailing bytes,
unknown enum values, and duplicate IDs. Strings are bounded before allocation.
No JSON, CBOR, base64 file data, nested structures, or implicit native-endian
fields occur on the TLS wire.

## Frames and limits

```text
u32 frame_len | u16 message_type | body
```

`frame_len` includes the two type bytes and body; it excludes its own four-byte
prefix. Read the six-byte header first. Reject invalid length/type before
allocating a body. The parser must handle split headers/bodies, coalesced frames,
EOF mid-frame, and cancellation. A valid header alone does not validate a body,
state, ID, digest, or consent. Every frame is inside verified TLS.

| Limit | Value |
| --- | --- |
| Control frame, including type | 65,536 bytes |
| Canonical manifest | 61,440 bytes |
| Files per wire offer | 1–128, subject to local limits |
| Filename | 1–255 UTF-8 bytes, with name rules below |
| MIME | 1–128 visible ASCII bytes, `type/subtype`; fallback `application/octet-stream` |
| DataChunk raw data | 1–262,144 bytes |
| DataChunk fixed body | 56 bytes: pair + transfer + file IDs, offset |
| Maximum frame_len | 262,202 = 2 + 56 + 262,144 |
| Queued data | At most four chunks; apply backpressure |
| File size | 0–1 TiB (1,099,511,627,776 bytes), known size required |
| Total offer size | At most 4 TiB, checked sum matching all entries |
| Concurrent unauthenticated connections | Two; never unbounded accept workers |

These are protocol ceilings, not promises of storage capacity or release support.
Local available-space, provider, and capability restrictions can reject smaller offers
before data. Unknown-size sources fail `UnsupportedFile`; preserve the draft.
No `u64` is converted to Kotlin Int or floating point. Android-facing values must
also fit signed Long; conversions fail rather than truncate.

## Message registry

`C` means `pair_id[16] || transfer_id[16]`. Both must match the active connection
and transfer. `F` means `C || file_id[16]`. S is sender, R is receiver. Bounds in
the table are **body bytes**, excluding both length and message type.

| Type | Name / direction | Body layout | Body bounds |
| --- | --- | --- | --- |
| 0x0001 | SessionHello S→R | major:u16, minor:u16, secure_session_id:ID, capabilities:u32=0 | 24 |
| 0x0002 | SessionHelloAck R→S | Same fields, exact accepted version/session | 24 |
| 0x0003 | PairAuthenticate S→R | invitation_id:ID, token[32], claimed_sender_name:text | 51–178 |
| 0x0004 | PairResult R→S | pair_id:ID, secure_session_id:ID | 32 |
| 0x0100 | TransferOffer S→R | C, manifest_digest[32], manifest_bytes:u32, manifest | 80–61,508 |
| 0x0101 | TransferAccept R→S | C, manifest_digest[32] | 64 |
| 0x0102 | TransferDecline R→S | C, manifest_digest[32], reason:u16 | 66 |
| 0x0200 | FileBegin S→R | F, offered_size:u64 | 56 |
| 0x0201 | FileReady R→S | F, next_offset:u64=0 | 56 |
| 0x0202 | DataChunk S→R | F, offset:u64, remaining body = raw data | 57–262,200 |
| 0x0203 | FileEnd S→R | F, actual_size:u64, BLAKE3_digest[32] | 88 |
| 0x0204 | FileVerified R→S | F, verified_size:u64, BLAKE3_digest[32] | 88 |
| 0x0205 | FileCommitted R→S | F, size:u64, digest[32], receipt_id:ID, actual_name:text | 107–361 |
| 0x0300 | TransferComplete R→S | C, manifest_digest[32], committed_count:u16, total_bytes:u64 | 74 |
| 0x0301 | CompleteAck S→R | Same summary | 74 |
| 0x0302 | TransferStatus R→S | F, phase:u8, phase_bytes:u64 | 57 |
| 0x7f00 | Cancel either direction | C, reason:u16 | 34 |
| 0x7fff | Error either direction | pair_id[16], transfer_id[16], code:u16, scope:u8 | 35 |

TransferOffer's minimum is only a structural bound (68 fixed bytes + 12 manifest
header bytes); the body decoder must additionally require at least one valid
entry. The remaining body must equal the declared manifest length exactly.
PairAuthenticate names are 1–32 Unicode scalar values and at most 128 bytes,
without C0/C1 controls; they are claimed names, not human identity proofs.

Decline reasons: 1 UserDeclined, 2 Busy, 3 UnsupportedFile, 4 StorageUnavailable.
Cancel reasons: 1 UserCancelled, 2 SessionEnded, 3 NetworkLost, 4 DeadlineExceeded.
Status phases: 1 AwaitingApproval, 2 PreparingStorage, 3 Verifying, 4 Saving,
5 AwaitingCompletion. Status file ID is zero only for a batch-wide phase.
Status cannot grant approval, extend an absolute deadline, or prove commit.

Error scope is 0 connection or 1 transfer. Before PairResult both IDs must be
zero; a paired connection-level error uses its pair ID and zero transfer ID.
A transfer error uses both active IDs. Never reflect peer strings or secrets.

| Error code | Meaning |
| --- | --- |
| 0x0001 | UnsupportedVersion / capability |
| 0x0002 | MalformedFrame / type / body |
| 0x0003 | LimitExceeded |
| 0x0004 | UnexpectedState / direction / ID |
| 0x0005 | AuthenticationFailed; generic remote reason |
| 0x0006 | SessionExpired |
| 0x0007 | Busy |
| 0x0008 | UnsupportedFile |
| 0x0009 | SourceUnavailable / source changed |
| 0x000a | StorageUnavailable |
| 0x000b | IntegrityMismatch |
| 0x000c | DeadlineExceeded |
| 0x000d | Cancelled |
| 0x000e | InternalFailure; no panic/path text |

Send at most one bounded Error if it is safe to do so, then close on a protocol
or authentication failure. Do not drain attacker-declared oversized bodies or
continue after state corruption. A network loss need not be writable as a frame.

## Canonical manifest and integrity

```text
manifest_schema:u16 = 1
file_count:u16
total_bytes:u64
repeat file_count times, in approved order:
    file_id:ID
    size:u64
    name:text
    mime:text
```

Reject zero/duplicate file IDs, missing/unknown sizes, count outside 1–128,
overflow, wrong total, oversize, and trailing data. Duplicate names are allowed;
the destination resolves conflicts without overwriting. Approval binds to the
exact entry order and original UTF-8 bytes, not a reserialized UI representation.

`manifest_digest = BLAKE3("NuvyroManifestV1\0" || canonical_manifest_bytes)`, using
ordinary unkeyed BLAKE3 with its 32-byte output. Domain text is ASCII including
the final NUL. FileEnd/FileVerified/FileCommitted use the ordinary 32-byte BLAKE3
of the actual file bytes, without this manifest prefix. Hashes travel only over
the same authenticated TLS connection. No custom cipher or chunk encryption.

Reject filename NUL/C0/C1, `/`, `\`, `.` or `..`, whitespace-only names, and bidi
overrides/isolates U+202A–U+202E and U+2066–U+2069. The name is one display
component, never a path. The local storage adapter chooses the actual safe name
under the selected tree. MIME must match the ASCII expression
`[A-Za-z0-9!#$&^_.+-]+/[A-Za-z0-9!#$&^_.+-]+`, without parameters or wildcards.
It is a hint and cannot trigger execution or opening. The actual destination
name in a receipt obeys the same filename bounds; fail finalization explicitly
if the provider cannot return a supported safe name.

Hashing authenticates neither the person nor the contents' safety. A source can
change while read; revalidate availability/size after consent and fail on size
change, read error, premature EOF, or extra bytes. Same-size mutation is not an
immutable snapshot guarantee. Resume and source snapshots are outside v1.

## State and consent

```text
TLS -> Hello -> InvitationAuthentication -> Paired
    -> Offer -> AwaitingApproval -> PreparingStorage -> Accepted
    -> FileBegin -> FileReady -> Streaming -> FileVerified -> Saving
    -> FileCommitted -> [next file] -> TransferComplete -> CompleteAck
```

- Only Hello is accepted after TLS. Then authenticate one invitation. A second
  hello/authentication, reversed direction, wrong generation/session/ID, or
  unknown message is a protocol failure, never an implicit restart.
- Pairing is connection-local. A new connection requires a new invitation and
  pairing, never the consumed token as a resume credential.
- Show the actual manifest and claimed sender to the receiver. Approve/Decline
  binds pair + connection generation + transfer ID + manifest digest. A changed
  name/count/size/order requires a new offer and consent. No auto-accept.
- Approval alone is not TransferAccept: first preflight folder access, limits,
  staging capacity, and storage policy. Only then send TransferAccept. No source
  file data read for transfer or destination write before explicit approval.
- On FileBegin validate ID/order/size, create the staging destination, and send
  FileReady only when ready. Offset must be zero initially and then exactly the
  accumulated bytes. Reject duplicates, gaps, excess bytes, and early FileEnd.
- A zero-byte file has FileBegin, FileReady, no DataChunk, and FileEnd with the
  BLAKE3 empty-stream digest. All files still require verification and commit.
- Sender waits for FileCommitted before beginning the next file. V1 stops on
  the first failure; earlier committed files remain and partial results are
  reported. Parallel files and continuing after a failed file require a revision.
- Cancel/expiry closes the active transaction and cleans uncommitted data best
  effort. A late event cannot revive state. A commit racing cancel remains a
  committed per-file result; cancel never deletes a committed document.

## Deadlines and queues

Use monotonic time locally, never a peer-provided wall-clock expiry.
TLS handshake 10 s; hello and invitation authentication each 10 s; invitation
120 s from receiver issuance; consent 60 s from completed offer; storage
preparation/FileReady 60 s; valid-frame network inactivity 30 s. Receiver emits
phase status at most once per 5 s while waiting/verifying/saving; it does not
extend approval, invitation, or absolute phase deadlines.

For streaming, verifying, and saving separately, the absolute phase deadline in
seconds is `min(86400, max(300, 60 + ceil(file_size / 262144)))`. This explicit v1
policy can fail a slow provider; do not show success or silently wait forever.
The idle deadline also applies while waiting for phase status. Invalid frames
do not refresh it. Paired inactivity without an offer closes after 300 s.

Limit new TLS attempts to eight per receiver per rolling 60 s, maximum two
concurrent handshakes. Limit failed invitation checks to five per invitation per
rolling 60 s, then invalidate it. Rate limits cannot eliminate hostile LAN DoS.
One outstanding invitation per Receive session; creating another invalidates the
previous unused invitation, never a currently paired transfer. Queues and workers
remain bounded; cancellation does not make a blocked provider instantly stop.

## Storage, receipt, and truthful completion

1. Keep active staging in app-private no-backup files, not evictable cache. After
   consent require local staging space for the file plus a 16 MiB reserve; check
   destination capacity when available but still handle mid-write failures.
2. Stream with bounded buffers and incremental BLAKE3. After exact size/hash
   match send FileVerified. It means verified staging, not saved destination.
3. Export to a newly created SAF document under the chosen tree. Never overwrite.
   Temporary naming/rename is provider-dependent and not assumed atomic. A partial
   document may be visible while Saving; retain cleanup bookkeeping on failure.
4. Close/flush the destination as supported, reopen it read-only, and verify its
   size and BLAKE3. Providers that cannot satisfy create/write/reopen/readback are
   unsupported in v1. Validation flags alone do not prove these operations work.
5. Write a per-file receipt with Android AtomicFile in app-private no-backup
   storage, binding pair/transfer/file/manifest digest, size/hash, receipt ID,
   actual name, and local document URI. URI never goes on the wire. Then tell
   core to emit FileCommitted and clean staging when safe.
6. TransferComplete is sent only after every file has a recorded commit. Its
   count/total/digest and CompleteAck must match the offer and all receipts.

Receiver may mark local files completed once destination and journal succeed;
peer acknowledgement is a separate flag. Sender marks batch Completed only after
all FileCommitted messages and the matching TransferComplete, then attempts
CompleteAck. Loss of the final acknowledgement does not undo committed files.
Missing receipt/batch confirmation after data may have been saved produces
DeliveryUnconfirmed with confirmed per-file results, not automatic resend or
an invented success. Reconciliation requires fresh authentication.

SAF and the local receipt are not one atomic transaction. A crash between export
and receipt can leave a file without a journal entry. Do not promise exactly-once,
atomic batches, arbitrary provider support, or power-loss durability. Never
automatically execute/open a received file. Permission loss, full disk, SD removal,
and provider errors are explicit outcomes; do not reuse a peer-supplied path.

## Test vectors

`tests/fixtures/frame-headers.txt` defines header allocation-bound cases.
`contract-vectors.json` contains canonical QR/manifest encodings and complete
frame examples. The Python checker validates artifact encodings independently
of Rust; it does not test TLS, consent, or storage behavior. The empty-file hash
vector comes from the official BLAKE3 project. Fixture IDs, pins, and tokens are
public deterministic examples, never usable session secrets.
