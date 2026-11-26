# LAN discovery and profile exchange

Nuvyro uses Android Network Service Discovery (mDNS/DNS-SD) to find receivers on
the same Wi-Fi network. Kotlin calls the Android APIs directly, within the app
process. This implementation does not require AIDL or a Rust toolchain.

## Availability and discovery

- Receive advertises `_nuvyro._tcp.` while its screen is resumed.
- Each session uses a random UUID and a service name of `Nuvyro-<UUID>`.
- TXT attributes are `v=1` and `id=<session UUID>`.
- The advertised TCP port serves only a bounded profile response. It does not
  accept files, transfer requests, filesystem paths, or commands.
- Android 33 and later scope NSD to the selected Wi-Fi network. Earlier versions
  use the system discovery API and filter resolved addresses against the Wi-Fi
  link prefixes. Outgoing sockets bind to the selected Android `Network`.
- Incoming requests must reach a local address on that Wi-Fi network and come
  from one of its link prefixes. The server accepts both address families on its
  ephemeral port, rejecting connections through other interfaces before reading
  or returning profile data.
- Android 17/API 37 requires `ACCESS_LOCAL_NETWORK` at runtime. The app asks from
  Send/Receive. `INTERNET` and `ACCESS_NETWORK_STATE` are manifest permissions;
  using this LAN feature does not require an Internet connection.

NSD discovery must report that it started before the radar enters `Searching`.
Missing permission, missing Wi-Fi, startup, pause, and failure have separate UI
states. Backgrounding or leaving the screen cancels network callbacks, NSD,
profile polling, and the profile server. Receive becomes available again when
its screen resumes. NSD startup has a ten-second timeout and a retry action on
failure. No foreground service is started.

The browser tracks up to 32 services, serializes NSD resolution, and permits up
to four concurrent profile checks. Known services are checked every eight
seconds after each check completes. NSD loss removes a receiver immediately;
two consecutive failed checks also remove it until it responds again. Selected
receivers are cleared when their IDs disappear.

## Profile protocol version 1

All integers use network byte order. A request contains only the 32-bit magic
value `0x4e565031` (`NVP1`). The server returns:

| Field | Encoding | Constraint |
| --- | --- | --- |
| Magic | 32-bit integer | `0x4e565031` |
| Version | 32-bit integer | `1` |
| Session ID | Unsigned 16-bit byte length, then UTF-8 | Canonical UUID; at most 36 bytes |
| Display name | Unsigned 16-bit byte length, then UTF-8 | At most 128 bytes and 1–32 Unicode code points; no control characters |
| Photo length | 32-bit integer | 0–32,768 bytes |
| Photo | Raw bytes | Empty or a small JPEG avatar |

The client checks the packet session ID against the TXT attribute. UTF-8 must
decode strictly. Remote images are inspected for dimensions before decoding and
must be at most 256 × 256 pixels. Locally published avatars are 128 × 128 JPEGs
without source metadata. The server uses the saved local profile, including the
device-model fallback when the user skipped profile setup.

Sockets have a two-second read timeout. The client closes each connection after
four seconds overall or on cancellation. The server permits four clients and
closes each request after three seconds. The listener checks cancellation at
least every 500 ms and closes accepted sockets during shutdown.

## Trust boundary and next stage

Discovery and profile exchange are unauthenticated and unencrypted. They expose
only the name and avatar the receiving user chooses to make visible on the LAN.
A displayed name or photo is not proof of the peer's identity. The bounds and
timeouts above limit profile handling; they do not authenticate devices.

File transfer must add pairing/authentication, receiver consent, encrypted
transport, integrity verification, and explicit completion semantics before
files are accepted. A future transfer engine, including a possible Rust engine,
can use the discovered endpoint after that protocol is defined. The profile
endpoint must not silently become an unauthenticated file-transfer endpoint.

Android references:
[NSD](https://developer.android.com/develop/connectivity/wifi/use-nsd),
[local network permission](https://developer.android.com/privacy-and-security/local-network-permission),
[AIDL](https://developer.android.com/develop/background-work/services/aidl).
