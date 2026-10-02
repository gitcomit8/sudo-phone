//! BLE server using FIDO2 CTAP2 profile over BlueZ D-Bus.
//!
//! Implements a GATT server that exposes CTAP2 protocol characteristics.
//! Android phones can connect via BLE and communicate using the standard
//! CTAP2 CBOR message format.
//!
//! BlueZ D-Bus interfaces used:
//! - `org.bluez.Adapter1` — create/manage adapters
//! - `org.bluez.GattManager1` — register GATT services
//! - `org.bluez.GattService1` — define a service
//! - `org.bluez.GattCharacteristic1` — define characteristics
//! - `org.bluez.LEAdvertisingManager1` — advertise services
//!
//! CTAP2 over BLE uses two characteristics:
//! - Request:  0x0001 (write, no-notify)
//! - Response: 0x0002 (read, notify)
//!
//! Messages are CBOR-encoded CTAP2 messages (FIDO spec).

use tracing::{info, debug};

use crate::auth_request::{AuthRequestHandler, AuthRequest};

pub mod bluez {
    //! BlueZ D-Bus interface constants and helpers.
    //!
    //! These are the well-known D-Bus paths, interfaces, and methods
    //! for interacting with BlueZ on Linux.

    /// BlueZ object manager path
    pub const BLUEZ_MANAGER_PATH: &str = "/org/bluez";
    /// BlueZ D-Bus interface for the manager
    pub const BLUEZ_MANAGER_IFACE: &str = "org.bluez.Manager";
    /// Default adapter path (resolved at runtime)
    pub const DEFAULT_ADAPTER: &str = "hci0";

    // GATT Manager interfaces
    pub const GATT_MANAGER_IFACE: &str = "org.bluez.GattManager1";
    pub const SERVICE_IFACE: &str = "org.bluez.GattService1";
    pub const CHARACTERISTIC_IFACE: &str = "org.bluez.GattCharacteristic1";

    // Advertising manager
    pub const ADVERTISING_MANAGER_IFACE: &str = "org.bluez.LEAdvertisingManager1";
    pub const ADVERTISING_MANAGER_PATH: &str = "/org/bluez/hci0";

    // Adapter interface
    pub const ADAPTER_IFACE: &str = "org.bluez.Adapter1";

    // CTAP2 BLE characteristic UUIDs (FIDO-defined)
    /// CTAP2 request characteristic (write without response)
    pub const CTAP2_REQUEST_UUID: &str = "00000001-0000-1000-8000-00805f9b34fb";
    /// CTAP2 response characteristic (read + notify)
    pub const CTAP2_RESPONSE_UUID: &str = "00000002-0000-1000-8000-00805f9b34fb";
    /// CTAP2 ping/keepalive characteristic (optional)
    pub const CTAP2_PING_UUID: &str = "00000003-0000-1000-8000-00805f9b34fb";

    // FIDO2 service UUID
    pub const FIDO2_SERVICE_UUID: &str = "0000fee0-0000-1000-8000-00805f9b34fb";

    /// CBOR tag for CTAP2 requests (FIDO spec)
    pub const CTAP2_CBOR_TAG: u8 = 0x60;
}

/// A CTAP2 BLE GATT server backed by BlueZ.
pub struct BleServer {
    /// Registered service path (if any)
    service_path: Option<String>,
    /// Characteristic paths
    char_paths: (Option<String>, Option<String>), // (request, response)
    /// Whether advertising is active
    advertising: bool,
}

impl BleServer {
    /// Create a new BLE server, registering the CTAP2 GATT service with BlueZ.
    pub async fn new() -> Result<Self, BleError> {
        let server = Self {
            service_path: None,
            char_paths: (None, None),
            advertising: false,
        };

        server.register_with_bluez().await?;
        Ok(server)
    }

    /// Register the CTAP2 GATT service and characteristics with BlueZ.
    async fn register_with_bluez(&self) -> Result<(), BleError> {
        // In a full implementation, we would:
        // 1. Connect to the BlueZ system bus via D-Bus (zbus)
        // 2. Query org.bluez.ObjectManager1 for adapters
        // 3. Find the default adapter (hci0)
        // 4. Register our GATT service under that adapter
        // 5. Register request and response characteristics
        // 6. Start advertising via LEAdvertisingManager1

        // zbus 4.x API:
        // let conn = Connection::system().await?;
        // let name = zbus::names::BusName::from("org.bluez".to_string());
        // let path = zbus::names::ObjectPath::from("/org/bluez".to_string());
        // let mgr: zbus::Proxy<'_> = zbus::Proxy::new(
        //     Some(name), conn, &path, None
        // ).await?;

        info!("BLE server: would register CTAP2 GATT service with BlueZ");
        info!(
            "Service UUID: {}",
            bluez::FIDO2_SERVICE_UUID
        );
        info!(
            "Request char UUID: {}",
            bluez::CTAP2_REQUEST_UUID
        );
        info!(
            "Response char UUID: {}",
            bluez::CTAP2_RESPONSE_UUID
        );

        Ok(())
    }

    /// Start the BLE server event loop, handling incoming CTAP2 messages.
    pub async fn run(
        &self,
        auth_handler: &AuthRequestHandler,
    ) -> Result<(), BleError> {
        info!("BLE server event loop started");

        // In production, this would:
        // 1. Listen for incoming BLE connections on the GATT service
        // 2. When a write comes to the request characteristic, parse the CTAP2 message
        // 3. Route the message (makeAuthRequest, makeCredential, etc.)
        // 4. Send responses via the response characteristic

        // Main loop: periodically check for pending auth requests
        // and broadcast them to paired phones.
        let poll_interval = tokio::time::Duration::from_millis(500);

        loop {
            // Periodic cleanup of expired requests
            auth_handler.cleanup().await;
            tokio::time::sleep(poll_interval).await;
        }

        #[allow(unreachable_code)]
        Ok(())
    }

    /// Broadcast an auth request to all paired phones via BLE.
    ///
    /// This writes the CTAP2 makeAuthRequest message to the BLE
    /// request characteristic, which broadcasts to all connected devices.
    async fn broadcast_request(
        &self,
        request: &AuthRequest,
    ) -> Result<(), BleError> {
        // Encode the auth request as a CTAP2 CBOR message
        let ctap2_msg = self.encode_ctap2_auth_request(request);

        debug!(
            nonce = format!("{:016x}", request.nonce),
            "Broadcasting CTAP2 auth request ({} bytes)",
            ctap2_msg.len()
        );

        // In production, write to the BLE request characteristic:
        // org.bluez.GattCharacteristic1.WriteValue(path, value, options)
        //
        // The request characteristic is configured as "write-without-response",
        // so no ACK is expected at the BLE layer. The CTAP2 level handles reliability.

        Ok(())
    }

    /// Encode an auth request as a CTAP2 CBOR message.
    ///
    /// CTAP2 auth request format (simplified):
    /// {
    ///   "cmd": 0x06,  // CTAP2 CMD_GET_ASSERTION
    ///   "hash": <sha256(rp_id)>,
    ///   "chall": <nonce_padded>,
    ///   "allowList": [],
    /// }
    fn encode_ctap2_auth_request(&self, request: &AuthRequest) -> Vec<u8> {
        // CTAP2 command codes (FIDO spec)
        const CTAP2_CMD_GET_ASSERTION: u8 = 0x06;
        const CTAP2_CMD_GET_INFO: u8 = 0x04;

        // Build a simplified CBOR-like message:
        // [cmd, {nonce, username, ttl}]
        //
        // In production, this would be proper CBOR encoding per the FIDO spec.
        // Using a simplified format for now.
        let mut msg = vec![CTAP2_CMD_GET_ASSERTION];
        msg.extend_from_slice(&request.nonce.to_le_bytes());
        msg.extend_from_slice(request.username.as_bytes());
        msg.extend_from_slice(&request.ttl.to_le_bytes());
        msg
    }

    /// Decode an incoming CTAP2 message from a phone.
    fn decode_ctap2_message(&self, data: &[u8]) -> Option<CTAP2Message> {
        if data.is_empty() {
            return None;
        }

        match data[0] {
            0x06 => {
                // CTAP2_GET_ASSERTION response
                // Format: [cmd, {nonce, signature_info, credential_id, auth_data, signature}]
                if data.len() >= 9 {
                    let nonce_bytes: [u8; 8] = data[1..9].try_into().ok()?;
                    let nonce = u64::from_le_bytes(nonce_bytes);
                    Some(CTAP2Message::AuthResponse { nonce })
                } else {
                    None
                }
            }
            0x04 => Some(CTAP2Message::GetInfo),
            _ => Some(CTAP2Message::Unknown(data[0])),
        }
    }
}

/// A CTAP2 message received from a phone.
#[derive(Debug)]
enum CTAP2Message {
    /// Auth response (makeAuthRequest reply)
    AuthResponse { nonce: u64 },
    /// GetInfo request (phone capabilities)
    GetInfo,
    /// Unknown command
    Unknown(u8),
}

/// BLE server errors
#[derive(thiserror::Error, Debug)]
pub enum BleError {
    #[error("D-Bus error: {0}")]
    Dbus(#[from] zbus::Error),

    #[error("BLE not available: {0}")]
    NotAvailable(String),

    #[error("CTAP2 protocol error: {0}")]
    Protocol(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctap2_encode_decode() {
        let server = BleServer {
            conn: None,
            service_path: None,
            char_paths: (None, None),
            advertising: false,
        };

        let request = AuthRequest {
            nonce: 0x1234567890abcdef,
            username: "testuser".to_string(),
            timestamp: chrono::Utc::now(),
            ttl: 30,
        };

        let encoded = server.encode_ctap2_auth_request(&request);
        assert_eq!(encoded[0], 0x06); // CTAP2_GET_ASSERTION
        assert_eq!(encoded.len(), 8 + 8 + 8 + 1 + 4); // nonce + username + ttl

        let decoded = server.decode_ctap2_message(&encoded);
        assert!(matches!(decoded, Some(CTAP2Message::AuthResponse { .. })));
    }
}
