# CLAUDE.md — Phone Auth Project Guide

## Project Overview

Biometric sudo via Bluetooth LE. The phone's fingerprint **never leaves the phone**.
The phone sends a signed "verified" token (P-256 HMAC) to a Rust daemon on the laptop,
which accepts it via PAM for `sudo`/login.

**Security model:**
- Fingerprint biometric stays on the Android device (BiometricPrompt API)
- Phone signs `{nonce, username, timestamp}` with a shared P-256 pairing key
- Daemon verifies signature + nonce freshness (TTL-based replay protection)
- Daemon runs as root (required for PAM integration)

## Architecture

```
Phone (Flutter)                          Laptop (Rust)
┌──────────────┐                         ┌──────────────────┐
│ BiometricPrompt│──verify──▶  ✅        │  BLE Daemon      │
│   (Android)   │    "user verified"     │  (root)          │
└──────────────┘    ──────────▶         │  ├─ BLE server   │
     fingerprint NEVER leaves            │  ├─ Pairing mgr  │
     the phone                           │  └─ PAM bridge   │
```

**Protocol:** FIDO2 CTAP2 over BLE. Same protocol Android uses for hardware security keys.
Battle-tested, standardized, widely supported.

## Project Structure

```
phone-auth/
├── rust/daemon/                  # Rust daemon (laptop, runs as root)
│   ├── src/
│   │   ├── main.rs               # Entry point, CLI (clap subcommands)
│   │   ├── ble/mod.rs            # BLE server — CTAP2 GATT service over BlueZ D-Bus
│   │   ├── pairing.rs            # Key exchange, device registry (pairs.json)
│   │   ├── pam.rs                # PAM integration (pam_exec helper)
│   │   └── auth_request.rs       # Auth request queue, timeout, nonce handling
│   └── Cargo.toml
├── flutter/app/                  # Flutter phone app (Android only)
│   ├── lib/
│   │   ├── main.dart             # App entry, provider setup
│   │   ├── ble/ble_client.dart   # BLE client — connects to laptop GATT service
│   │   ├── auth_handler.dart     # Biometric prompt + response signing
│   │   ├── model/auth_request.dart  # AuthRequest / AuthResponse data models
│   │   └── ui/
│   │       ├── home_screen.dart  # Connection status, pair button
│   │       └── pairing_screen.dart  # Scan + connect to laptop
│   ├── android/                  # Android project (minSdk 23+)
│   └── pubspec.yaml
├── setup-pam.sh                  # PAM + sudoers configuration script
├── phone-auth.service            # Systemd unit file
├── README.md                     # User-facing setup guide
└── CLAUDE.md                     # This file — developer/AI guide
```

## Key Files

| File | Purpose |
|------|---------|
| `rust/daemon/src/main.rs` | CLI entry, daemonize(), run_daemon(), subcommands (start/pair/list/remove/unpair-all/test) |
| `rust/daemon/src/ble/mod.rs` | BleServer struct, BlueZ D-Bus constants (CTAP2 UUIDs), GATT service registration stub |
| `rust/daemon/src/pairing.rs` | PairingStore (server_key + devices[]), load/save pairs.json, add/remove devices |
| `rust/daemon/src/pam.rs` | pam_authenticate(), pam_exec helper, PAM config snippets, install_pam_config() |
| `rust/daemon/src/auth_request.rs` | AuthRequestHandler (pending requests, response map, Notify), verify_signature() |
| `flutter/app/lib/main.dart` | Flutter app entry, MultiProvider (BleClient + AuthHandler) |
| `flutter/app/lib/ble/ble_client.dart` | FlutterBluePlus connection, GATT discovery, request/response handling |
| `flutter/app/lib/auth_handler.dart` | LocalAuthentication biometric prompt, response signing |

## Current State

### Rust Daemon — COMPILED ✅
```bash
cd rust/daemon && cargo build
```
- Clean build, 30 dead-code warnings (expected — modules exist but aren't wired together yet)
- No compilation errors

### Flutter App — SCAFFOLDED ✅
- All source files created
- Android manifest with BLE + biometric permissions
- pubspec.yaml with dependencies (flutter_blue_plus, local_auth, provider, etc.)
- Not yet tested with `flutter analyze` (Flutter SDK may not be installed)

### Infrastructure — CREATED ✅
- `setup-pam.sh` — installs PAM helper, modifies /etc/pam.d/sudo, creates sudoers drop-in
- `phone-auth.service` — systemd unit with security hardening
- `.gitignore` — excludes target/, build/, config files with keys

## Dependencies

### Rust (Cargo.toml)
| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime (full features) |
| `zbus` | BlueZ D-Bus communication |
| `k256` | P-256 elliptic curve crypto (ECDSA key exchange) |
| `sha2` + `hmac` | HMAC-SHA256 for token signing |
| `clap` | CLI argument parsing |
| `serde` + `serde_json` | JSON serialization for pairs.json |
| `thiserror` | Error derive macro |
| `tracing` + `tracing-subscriber` | Structured logging |
| `chrono` | Timestamps |
| `rand` | Nonce generation |
| `uuid` | UUID v4 generation |
| `anyhow` | Top-level error type |
| `libc` | Daemonization (fork, dup2) |

### Flutter (pubspec.yaml)
| Package | Purpose |
|---------|---------|
| `flutter_blue_plus` | BLE scanning, connection, GATT |
| `local_auth` | Android BiometricPrompt |
| `provider` | State management |
| `pointycastle` | Crypto primitives (P-256, HMAC) |
| `crypto` | HMAC-SHA256 |
| `json_annotation` | JSON serialization |

## CTAP2 BLE Protocol

### GATT Service UUID
```
Service:  0000fee0-0000-1000-8000-00805f9b34fb  (FIDO2)
Request:  00000001-0000-1000-8000-00805f9b34fb  (write, no-notify)
Response: 00000002-0000-1000-8000-00805f9b34fb  (read, notify)
```

### Message Format (simplified)
```
Auth Request:  [cmd=0x06, nonce(8 bytes LE), username..., ttl(4 bytes LE)]
Auth Response: [cmd=0x06, nonce(8 bytes LE), signature(32 bytes HMAC)]
```

### CTAP2 Command Codes
- `0x04` — GET_INFO (phone capabilities)
- `0x06` — GET_ASSERTION (auth request)
- `0x02` — MAKE_CREDENTIAL (registration)

## Pairing Flow

1. Laptop runs `phone-auth-daemon pair` → generates server key, prints prefix
2. Phone app scans for BLE service (UUID 0xfee0)
3. Phone connects, exchanges public keys over BLE
4. Both sides store the other's P-256 public key
5. Laptop stores phone's key in `/etc/phone-auth/pairs.json`
6. Future auth: phone signs with its private key, laptop verifies with stored public key

## PAM Integration

### pam_exec approach (primary)
```
/etc/pam.d/sudo:
  auth sufficient /usr/lib/security/pam_exec.so quiet /usr/local/bin/phone-auth-pam-helper
  auth required pam_deny.so  # fallback
```

### sudoers drop-in
```
/etc/sudoers.d/phone-auth:
  Defaults timestamp_timeout=5
  Defaults timestampdir="/run/sudo/ts"
```

### Helper script flow
1. PAM calls helper with PAM_USER env var
2. Helper connects to daemon via Unix socket
3. Daemon broadcasts auth request to phone
4. Phone shows biometric prompt
5. Phone signs and sends response
6. Daemon verifies, returns "accepted" or "error"
7. Helper exits 0 (accepted) or 1 (rejected)

## Security Considerations

- **Nonces prevent replay attacks** — each auth request has a unique 64-bit nonce
- **TTL-based expiration** — requests expire after 30 seconds
- **Nonce reuse detection** — daemon tracks used nonces, rejects duplicates
- **P-256 signing** — phone signs tokens with pairing key, daemon verifies
- **pairs.json permissions** — mode 0600 (root-only read/write)
- **Daemon runs as root** — required for PAM, but minimal attack surface
- **Fingerprint never leaves phone** — BiometricPrompt API handles all biometric processing on-device

## Development Commands

### Rust
```bash
cd rust/daemon
cargo check          # Fast syntax/type check
cargo build          # Debug build
cargo build --release # Optimized release build
cargo test           # Run tests
cargo clippy         # Lint
```

### Flutter
```bash
cd flutter/app
flutter analyze      # Static analysis
flutter build apk    # Build Android APK
flutter run          # Run on connected device
```

### Install
```bash
# Build and install daemon
cd rust/daemon && cargo build --release
sudo cp target/release/phone-auth-daemon /usr/local/bin/

# Setup PAM
sudo ./setup-pam.sh

# Start daemon
sudo phone-auth-daemon start
# Or as systemd service:
sudo cp phone-auth.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now phone-auth.service
```

## TODO / Next Steps

1. **BlueZ GATT registration** — implement actual D-Bus calls in `ble/mod.rs`
2. **Flutter app wiring** — connect BLE client to auth handler, wire biometric flow
3. **Crypto implementation** — actual P-256 key exchange and HMAC-SHA256 verification
4. **End-to-end auth** — PAM → daemon → BLE → phone → verify → PAM accept
5. **Testing** — integration tests for the full flow
6. **Hardening** — error handling, edge cases, race conditions

## Common Issues

- **BlueZ not running:** `sudo systemctl start bluetooth`
- **Permission denied on BLE:** user must be in `bluetooth` group, or run as root
- **PAM config errors:** test with `visudo -c` and check `/var/log/auth.log`
- **Flutter BLE permissions:** Android 12+ requires BLUETOOTH_SCAN, BLUETOOTH_CONNECT runtime permissions
- **BiometricPrompt not available:** device must have fingerprint/face unlock configured
