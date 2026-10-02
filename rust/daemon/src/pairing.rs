//! Pairing management — key exchange, storage, and device registry.
//!
//! Each paired device stores:
//! - A public key (secp256r1 / P-256) used to verify "verified" tokens
//! - An alias for human identification
//! - Last-seen timestamp
//!
//! The pairing key is exchanged once via the BLE pairing flow:
//! 1. Laptop generates an ephemeral P-256 keypair
//! 2. Phone generates its own P-256 keypair
//! 3. Both sides exchange public keys over BLE (encrypted channel)
//! 4. Each side stores the other's public key for future token verification

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;
use tracing::info;

use rand::{thread_rng, Rng};

#[derive(Error, Debug)]
pub enum PairingError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid pairing data: {0}")]
    InvalidData(String),

    #[error("No pairing key for device index: {0}")]
    NoSuchDevice(usize),

    #[error("Pairing store not found")]
    NotFound,
}

/// A single paired device entry
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PairedDevice {
    /// Human-readable alias (e.g., "amb-s24")
    pub alias: String,
    /// Device's public key (P-256, uncompressed point, hex-encoded)
    pub public_key: String,
    /// Last time we saw/communicated with this device
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
    /// When the device was first paired
    pub paired_at: chrono::DateTime<chrono::Utc>,
}

/// The full pairing store (all paired devices + our own server key)
#[derive(Serialize, Deserialize, Debug)]
pub struct PairingStore {
    /// Our server's persistent signing key (used to sign challenges to the phone)
    pub server_key: String,
    /// Paired devices indexed by their public key hash
    pub devices: Vec<PairedDevice>,
}

impl PairingStore {
    pub fn new() -> Self {
        Self {
            server_key: hex::encode(generate_random_bytes(32)),
            devices: Vec::new(),
        }
    }

    /// Find a device by its public key (hex-encoded)
    pub fn find_by_key(&self, public_key_hex: &str) -> Option<&PairedDevice> {
        self.devices.iter().find(|d| d.public_key == public_key_hex)
    }

    /// Update last-seen timestamp for a device
    pub fn touch_device(&mut self, public_key_hex: &str) {
        if let Some(device) = self
            .devices
            .iter_mut()
            .find(|d| d.public_key == public_key_hex)
        {
            device.last_seen = Some(chrono::Utc::now());
        }
    }
}

impl Default for PairingStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Load the pairing store from disk
pub fn load_pairs(config_dir: &Path) -> Result<PairingStore, PairingError> {
    let path = config_dir.join("pairs.json");
    if !path.exists() {
        return Err(PairingError::NotFound);
    }
    let data = fs::read_to_string(&path)?;
    let store: PairingStore = serde_json::from_str(&data).map_err(|e| {
        PairingError::InvalidData(format!("Failed to parse pairs.json: {e}"))
    })?;
    Ok(store)
}

/// Save the pairing store to disk
pub fn save_pairs(config_dir: &Path, store: &PairingStore) -> Result<(), PairingError> {
    let path = config_dir.join("pairs.json");
    let data = serde_json::to_string_pretty(store)?;
    // Write atomically: write to temp file then rename
    let tmp_path = config_dir.join("pairs.json.tmp");
    fs::write(&tmp_path, data)?;
    fs::rename(&tmp_path, &path)?;
    // Restrict permissions (root-only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Start the pairing flow: generate a server key if needed,
/// and prepare for key exchange with the phone.
pub async fn start_pairing(config_dir: &Path) -> Result<(), PairingError> {
    let mut store = load_pairs(config_dir).unwrap_or_else(|_| PairingStore::new());

    // Generate server key if not present
    if store.server_key.is_empty() {
        store.server_key = hex::encode(generate_random_bytes(32));
        info!("Generated new server key");
    }

    // In production, this would:
    // 1. Start BLE advertising with a pairing-specific UUID
    // 2. Wait for the phone app to connect
    // 3. Exchange public keys via a secure channel (FIDO2 CTAP2 cborAuthSymbols)
    // 4. Store the phone's public key
    //
    // For CLI pairing, we print the server public key hash for the user to verify
    // against the phone app, then the phone sends its public key.
    let server_key_hash = &store.server_key[..16];
    println!("Server key prefix: {}", server_key_hash);
    println!("Pairing mode: start the phone app and tap 'Pair'");
    println!("Verify the server key prefix matches on both devices");

    // Save the store (with server key if it was new)
    save_pairs(config_dir, &store)?;

    Ok(())
}

/// Add a paired device (called after successful key exchange)
pub fn add_device(
    config_dir: &Path,
    alias: String,
    public_key: String,
) -> Result<(), PairingError> {
    let mut store = load_pairs(config_dir)?;

    // Check for duplicate
    if store.find_by_key(&public_key).is_some() {
        return Err(PairingError::InvalidData("Device already paired".to_string()));
    }

    store.devices.push(PairedDevice {
        alias,
        public_key,
        last_seen: None,
        paired_at: chrono::Utc::now(),
    });

    save_pairs(config_dir, &store)?;
    info!("Paired new device");
    Ok(())
}

/// Generate random bytes
fn generate_random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    thread_rng().fill(&mut buf[..]);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pairing_store_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut store = PairingStore::new();
        store.devices.push(PairedDevice {
            alias: "test".to_string(),
            public_key: "03".repeat(32),
            last_seen: None,
            paired_at: chrono::Utc::now(),
        });
        save_pairs(dir.path(), &store).unwrap();

        let loaded = load_pairs(dir.path()).unwrap();
        assert_eq!(loaded.devices.len(), 1);
        assert_eq!(loaded.devices[0].alias, "test");
    }

    #[test]
    fn test_find_by_key() {
        let store = PairingStore::new();
        assert!(store.find_by_key("03".repeat(32).as_str()).is_none());

        let mut store = PairingStore::new();
        let key = "03".repeat(32);
        store.devices.push(PairedDevice {
            alias: "test".to_string(),
            public_key: key.clone(),
            last_seen: None,
            paired_at: chrono::Utc::now(),
        });
        assert!(store.find_by_key(&key).is_some());
    }
}
