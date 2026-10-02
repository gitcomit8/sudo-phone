# phone-auth

Biometric sudo via Bluetooth LE — fingerprint **never leaves the phone**.

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

**Security model:** The phone signs a `{nonce, username, timestamp}` token with
the shared P-256 pairing key. The daemon verifies the signature before accepting
the auth. Nonces prevent replay attacks.

## Quick Start

### Prerequisites (Laptop)

```bash
# Ubuntu/Debian — BlueZ, build tools
sudo apt install -y bluez libbluetooth-dev pkg-config gcc

# Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Build & Install

```bash
cd rust/daemon
cargo build --release

# Install daemon binary
sudo cp target/release/phone-auth-daemon /usr/local/bin/

# Create config directory (keys stored here)
sudo mkdir -p /etc/phone-auth
sudo chmod 700 /etc/phone-auth
```

### Pair with Phone

```bash
# Start pairing mode (prints server key prefix for verification)
phone-auth-daemon pair

# On the phone app, verify the key prefix matches, then confirm
# The phone's public key is stored in /etc/phone-auth/pairs.json
```

### Enable PAM Integration

```bash
# Run the setup script (installs helper, modifies PAM + sudoers)
sudo ./setup-pam.sh
```

### Start as Systemd Service

```bash
sudo cp phone-auth.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now phone-auth.service
```

## Project Structure

```
phone-auth/
├── rust/daemon/
│   ├── src/
│   │   ├── main.rs          # Entry point, CLI commands (clap)
│   │   ├── ble/mod.rs       # BLE server — CTAP2 GATT over BlueZ D-Bus
│   │   ├── pairing.rs       # Key exchange & storage (pairs.json)
│   │   ├── pam.rs           # PAM integration (pam_exec helper)
│   │   └── auth_request.rs  # Request queue / timeout / nonce handling
│   └── Cargo.toml
├── flutter/app/
│   ├── lib/
│   │   ├── main.dart        # App entry, provider setup
│   │   ├── ble/ble_client.dart   # BLE client — connects to laptop
│   │   ├── auth_handler.dart   # Biometric prompt + response signing
│   │   ├── model/auth_request.dart  # Data models
│   │   └── ui/
│   │       ├── home_screen.dart    # Connection status, pair button
│   │       └── pairing_screen.dart # Scan + connect to laptop
│   ├── android/             # Android project (minSdk 23+)
│   └── pubspec.yaml
├── setup-pam.sh             # PAM + sudoers configuration script
├── phone-auth.service       # Systemd unit file
├── README.md                # This file
└── CLAUDE.md                # Developer/AI guide (detailed architecture)
```

## CLI Commands

```bash
phone-auth-daemon start [--foreground]   # Start the daemon (default)
phone-auth-daemon pair                   # Start pairing mode
phone-auth-daemon list                   # List paired devices
phone-auth-daemon remove <index>         # Remove a paired device
phone-auth-daemon unpair-all             # Remove all paired devices
phone-auth-daemon test                   # One-shot auth test (debug)
```

## CTAP2 BLE Protocol

### GATT Service UUIDs

| Role | UUID |
|------|------|
| Service | `0000fee0-0000-1000-8000-00805f9b34fb` (FIDO2) |
| Request | `00000001-0000-1000-8000-00805f9b34fb` (write, no-notify) |
| Response | `00000002-0000-1000-8000-00805f9b34fb` (read, notify) |

### Message Format

```
Auth Request:  [cmd=0x06, nonce(8 bytes LE), username..., ttl(4 bytes LE)]
Auth Response: [cmd=0x06, nonce(8 bytes LE), signature(32 bytes HMAC)]
```

### CTAP2 Command Codes

| Code | Name | Direction |
|------|------|-----------|
| `0x02` | MAKE_CREDENTIAL | Phone → Laptop (registration) |
| `0x04` | GET_INFO | Laptop → Phone (capabilities) |
| `0x06` | GET_ASSERTION | Laptop → Phone (auth request) |

## Pairing Flow

1. Laptop runs `phone-auth-daemon pair` → generates server key, prints prefix
2. Phone app scans for BLE service (UUID `0xfee0`)
3. Phone connects, exchanges public keys over BLE
4. Both sides store the other's P-256 public key
5. Laptop stores phone's key in `/etc/phone-auth/pairs.json`
6. Future auth: phone signs with its private key, laptop verifies with stored public key

## PAM Integration

### pam_exec approach (primary)

Add to `/etc/pam.d/sudo` **before** other auth lines:

```
# Phone auth — biometric verification via paired phone
auth sufficient /usr/lib/security/pam_exec.so quiet /usr/local/bin/phone-auth-pam-helper
auth required pam_deny.so  # fallback
```

### sudoers drop-in

Create `/etc/sudoers.d/phone-auth`:

```
# Cache successful phone auth for 5 minutes
Defaults timestamp_timeout=5
Defaults timestampdir="/run/sudo/ts"
```

### Helper script flow

1. PAM calls helper with `PAM_USER` env var
2. Helper connects to daemon via Unix socket
3. Daemon broadcasts auth request to phone
4. Phone shows biometric prompt
5. Phone signs and sends response
6. Daemon verifies, returns "accepted" or "error"
7. Helper exits 0 (accepted) or 1 (rejected)

## Security Notes

- **Fingerprint never leaves the phone** — BiometricPrompt API handles all biometric processing on-device
- **Nonces prevent replay attacks** — each auth request has a unique 64-bit nonce
- **TTL-based expiration** — requests expire after 30 seconds
- **Nonce reuse detection** — daemon tracks used nonces, rejects duplicates
- **P-256 signing** — phone signs tokens with pairing key, daemon verifies
- **pairs.json permissions** — mode 0600 (root-only read/write)
- **Daemon runs as root** — required for PAM, minimal attack surface

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

## Troubleshooting

| Problem | Solution |
|---------|----------|
| BlueZ not running | `sudo systemctl start bluetooth` |
| Permission denied on BLE | User must be in `bluetooth` group, or run as root |
| PAM config errors | Test with `visudo -c` and check `/var/log/auth.log` |
| Flutter BLE permissions | Android 12+ requires BLUETOOTH_SCAN, BLUETOOTH_CONNECT runtime permissions |
| BiometricPrompt not available | Device must have fingerprint/face unlock configured |
| Daemon won't start | Check `/var/log/syslog` for errors, run with `RUST_LOG=debug` |

## Status

- [x] Rust daemon skeleton (CLI, pairing, auth queue, PAM module)
- [x] BLE server stub (BlueZ D-Bus constants, CTAP2 encoding)
- [ ] Full BlueZ GATT registration
- [ ] Flutter phone app wiring
- [ ] End-to-end auth flow
- [ ] Systemd service + PAM install scripts (tested)
