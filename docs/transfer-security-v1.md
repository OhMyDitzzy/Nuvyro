# Transfer v1 security contract

Security requirements for wire protocol 1.0. See `transfer-protocol-v1.md` for
exact layouts and limits. Pairing must fail closed for the negative cases below.

## Trust boundaries

Protect file contents/metadata in transit, bind consent to the offered data,
bound untrusted parsing/resource use, and avoid falsely confirmed saved files.
Assume a hostile LAN peer can advertise any name/avatar/service, alter routes,
open/race/replay connections, send malformed data, and disconnect at any point.
Discovery is public and unauthenticated. Android URI providers can fail or block.

The user must scan the receiver's current QR through a trusted visual channel.
A stolen photo of an unexpired QR grants an invitation attempt; TLS does not
identify a human or distinguish the thief from another token holder. V1 has no
permanent trusted-device identity. Compromised/rooted phones, malicious OS/provider,
traffic analysis, availability against a LAN jammer, and encryption at rest are
not solved here. Hashes are not malware detection or immutable source snapshots.

## TLS profile

- TLS 1.3 only, ALPN bytes `nuvyro-transfer/1` required on both peers. Disable
  early data/0-RTT, session tickets, resumption, and application auto-replay.
- Receiver creates a fresh session key and self-signed X.509 leaf certificate.
  The key/signature policy is P-256 ECDSA with SHA-256. The selected crypto
  provider must support it on the Android target.
- One leaf, no intermediates, DER at most 4 KiB. Require SAN DNS `nuvyro.local`,
  digitalSignature key usage, serverAuth EKU, CA=false, and the allowed key type.
  Use that fixed server name for TLS validation/SNI, not DNS resolution.
- Sender compares SHA-256 of DER SubjectPublicKeyInfo exactly with the scanned
  QR pin, checks the allowed certificate structure/name/self-signature, and
  validates the TLS CertificateVerify signature with the configured crypto
  provider. Replacing PKI trust with this explicit pin must never mean a verifier
  that returns success for all certificates or handshake signatures.
- Certificate notBefore is receiver wall time minus 300 s, notAfter is creation
  plus 86,400 s. Sender requires current wall time within validity; no hidden
  clock bypass. Device-time/certificate errors fail pairing with a useful local
  error. TLS and invitation deadlines still use monotonic time.
- Keep standard library checks for TLS transcript, Finished, algorithms, and
  record authentication. Do not add custom encryption, nonces, or a plaintext
  socket after pairing. TLS 1.2 verifier paths must reject if reached.
- Stop/recreate invalidates the session key/cert and unused invitations. A new
  invitation does not replace a live transfer's key. Expired certificates require
  a new session for new handshakes; no key persistence or Keystore trust is claimed.

## QR schema and invitation

Separate Wi-Fi joining from transfer pairing. Existing `WIFI:...` remains a
system-scanned network QR. Nuvyro's offline pairing scanner accepts only:

```text
NUVYRO:PAIR:1:<base64url-without-padding of exactly 104 bytes>
payload = wire_major:u16 || wire_minor:u16 || secure_session_id[16]
        || invitation_id[16] || SHA256_SPKI_pin[32] || random_token[32]
        || transfer_port:u16 || advertised_ttl_seconds:u16
```

The prefix is case-sensitive. Decoded major/minor must be 1/0, port 47739, and
TTL hint 120. Strictly reject padding, whitespace, alternate alphabets, noncanonical
encoding, all-zero IDs/pin/token, unknown prefix/version, wrong length, and total
QR text over 512 bytes before decoding. There is no URL/deep-link launch, hostname,
filesystem path, arbitrary endpoint, or QR upload. Camera is requested on use.
Scanned bytes are sensitive UI-session data, never preferences/saved state/logs.

Receiver creates a 16-byte invitation ID and 32-byte secret token with OS CSPRNG.
Its monotonic expiry is issuance + 120 s. TTL in QR is a hint, not a deadline
chosen by the sender; the invitation can expire sooner than a scan countdown.
After pinned TLS and version/session hello, sender transmits the invitation and
claimed name inside TLS. Receiver validates shape/state/session/expiry/token,
then atomically consumes the invitation and creates a pair ID. Concurrent valid
attempts have exactly one winner. A valid but disconnected attempt still consumes
the token; retry needs a new invitation. Invalid/expired/consumed tokens return
the same remote authentication failure, not a detailed token oracle.

Name/photo are claims. Sender authenticates the holder of the pinned session key;
receiver authenticates a holder of the one-time invitation. After pairing, a
separate user approval is still mandatory for the exact manifest and connection.
No file content or transfer destination write occurs before that approval.

## Failure cases and required evidence

| Attack/failure | Required behavior |
| --- | --- |
| Forged name/avatar/NSD, substituted endpoint | No transfer without QR pin, session match, token, and consent |
| Wrong SPKI, malformed cert, forged CertificateVerify, wrong ALPN/TLS version | Handshake fails, no file write |
| Wrong, expired, consumed, previous-session token | Generic auth failure; key/token never logged |
| Two simultaneous holders | Atomic token consume, one PairResult |
| New connection with old token/approval | Fresh invitation required; approval replay rejected |
| Data before approval, changed manifest, wrong IDs/direction/order | Protocol failure before opening transfer destination |
| Huge length, truncated/invalid UTF-8, duplicate/gapped chunk | Bounded rejection without large allocation or commit |
| Traversal/control/bidi filename, conflicting name | Reject unsafe metadata; local no-overwrite naming |
| Source mutation, extra/truncated bytes, wrong hash | No FileCommitted; explicit failure and partial cleanup |
| Lost network/Stop/cancel/stale worker | Generation invalidates late events; no revived session/QR |
| Permission loss/full disk/provider block/readback failure | No invented completion; bounded worker count and honest cleanup limits |
| Export succeeds but receipt/ACK is lost | Preserve committed data, DeliveryUnconfirmed when appropriate, no auto-resend |
| Secret retention/backups/logging | No key/token/QR in saved state, backups, analytics, or logs |

Keep keys/tokens native except the QR display/scan boundary; minimize copies and
use supported zeroization without promising every GC/library copy is erased.
Errors are stable typed codes, not exception dumps, URIs, paths, or file contents.
Code review, negative tests, device tests, dependency advisory review, and later
independent security review remain release gates. A compiler or header test alone
does not establish any of the handshake, consent, or storage properties above.

## References

- TLS 1.3 CertificateVerify and replay limits: RFC 8446, sections 4.4.3 and 8,
  `https://www.rfc-editor.org/rfc/rfc8446.html`.
- rustls certificate and TLS 1.3 signature verification responsibilities:
  `https://docs.rs/rustls/latest/rustls/client/danger/trait.ServerCertVerifier.html`.
- Android opaque Network handles and per-socket network binding:
  `https://developer.android.com/ndk/reference/group/networking`.
- Empty-stream hash fixture: official BLAKE3 `test_vectors/test_vectors.json`,
  `https://github.com/BLAKE3-team/BLAKE3/tree/master/test_vectors`.

These sources inform requirements. They do not audit this application contract
or establish implementation conformance.
