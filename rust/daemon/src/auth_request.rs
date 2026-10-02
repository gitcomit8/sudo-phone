//! Auth request queue and timeout handling.
//!
//! When a user runs `sudo`, the PAM module sends an auth request to the daemon.
//! The daemon queues the request, broadcasts it to all paired phones via BLE,
//! and waits for a signed "verified" token response.
//!
//! Request lifecycle:
//! 1. PAM module invokes daemon (via pam_exec) or signals via D-Bus
//! 2. Daemon creates an AuthRequest with a random nonce
//! 3. Request is broadcast to paired phones over BLE
//! 4. Phone shows biometric prompt, user verifies
//! 5. Phone signs {nonce, username, timestamp} with its pairing key
//! 6. Daemon verifies the signature and nonce
//! 7. If valid, PAM accepts the auth

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::{Mutex, Notify};
use tracing::{info, warn};

use crate::pairing::{PairingStore, PairedDevice};

/// A pending authentication request
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthRequest {
    /// Random nonce to prevent replay attacks
    pub nonce: u64,
    /// Username requesting auth
    pub username: String,
    /// When the request was created
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// TTL in seconds
    pub ttl: u64,
}

/// Response from the phone: a signed "verified" token
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthResponse {
    /// The nonce we sent (must match)
    pub nonce: u64,
    /// Username we sent (echoed back)
    pub username: String,
    /// Timestamp from the phone
    pub phone_timestamp: chrono::DateTime<chrono::Utc>,
    /// HMAC-SHA256 signature over {nonce, username, phone_timestamp}
    /// using the device's pairing key
    pub signature: String,
}

/// Result of processing an auth response
#[derive(Debug)]
pub enum AuthResult {
    /// Authenticated successfully
    Accepted,
    /// Signature verification failed
    Rejected(String),
    /// Request timed out (no response from phone)
    Timeout,
    /// Nonce mismatch (replay attack attempt)
    ReplayAttack,
}

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("No paired devices")]
    NoPairedDevices,

    #[error("Request expired: {0}")]
    Expired(String),

    #[error("Signature verification failed: {0}")]
    SignatureFailed(String),

    #[error("Timeout waiting for response")]
    Timeout,
}

/// Manages pending auth requests and their responses
pub struct AuthRequestHandler {
    /// All paired devices
    store: Mutex<PairingStore>,
    /// Pending requests: nonce -> AuthRequest
    pending: Mutex<HashMap<u64, AuthRequest>>,
    /// Response channel: nonce -> AuthResponse
    responses: Mutex<HashMap<u64, AuthResponse>>,
    /// Notify waiters when a response arrives
    response_notify: Notify,
}

impl AuthRequestHandler {
    pub fn new(store: PairingStore) -> Self {
        Self {
            store: Mutex::new(store),
            pending: Mutex::new(HashMap::new()),
            responses: Mutex::new(HashMap::new()),
            response_notify: Notify::new(),
        }
    }

    /// Create a new auth request and broadcast it to all paired phones
    pub async fn create_and_broadcast(
        &self,
        username: String,
        ttl_secs: u64,
    ) -> Result<u64, AuthError> {
        let store = self.store.lock().await;

        if store.devices.is_empty() {
            return Err(AuthError::NoPairedDevices);
        }

        let nonce = rand::random();

        // Log before moving username into the request
        info!(
            nonce = format!("{:016x}", nonce),
            user = &username,
            "New auth request, broadcasting to {} device(s)",
            store.devices.len()
        );

        let request = AuthRequest {
            nonce,
            username,
            timestamp: chrono::Utc::now(),
            ttl: ttl_secs,
        };

        // Store the pending request
        let mut pending = self.pending.lock().await;
        pending.insert(nonce, request.clone());

        // In production, this would broadcast the request to all paired phones
        // over BLE. The request contains the nonce and username.
        // broadcast_request(&store, &request).await;

        drop(pending);
        drop(store);

        Ok(nonce)
    }

    /// Process a response from a phone
    pub async fn handle_response(
        &self,
        response: AuthResponse,
    ) -> Result<AuthResult, AuthError> {
        // Check if we have a pending request for this nonce
        let pending = self.pending.lock().await;
        let request = match pending.get(&response.nonce) {
            Some(r) => r.clone(),
            None => {
                return Ok(AuthResult::Rejected(
                    "Unknown nonce — possible replay attack".to_string(),
                ));
            }
        };

        // Check for nonce reuse (replay attack)
        if self.responses.lock().await.contains_key(&response.nonce) {
            warn!(
                nonce = format!("{:016x}", response.nonce),
                "Replay attack detected: nonce already used"
            );
            return Ok(AuthResult::ReplayAttack);
        }

        // Check TTL
        let elapsed = chrono::Utc::now()
            .signed_duration_since(request.timestamp)
            .num_seconds() as u64;
        if elapsed > request.ttl {
            warn!(
                nonce = format!("{:016x}", response.nonce),
                elapsed,
                "Auth request expired"
            );
            return Ok(AuthResult::Rejected(format!(
                "Request expired after {elapsed}s (TTL: {}s)",
                request.ttl
            )));
        }

        // Verify username matches
        if response.username != request.username {
            return Ok(AuthResult::Rejected("Username mismatch".to_string()));
        }

        // Verify signature (in production, using the device's pairing key)
        // For now, just record the response
        let mut responses = self.responses.lock().await;
        responses.insert(response.nonce, response);

        // Notify any waiters
        drop(responses);
        self.response_notify.notify_one();

        Ok(AuthResult::Accepted)
    }

    /// Wait for a response to a specific nonce, with timeout.
    ///
    /// Polls the response map until a response arrives or the timeout fires.
    pub async fn wait_for_response(
        &self,
        nonce: u64,
        timeout_secs: u64,
    ) -> Result<AuthResult, AuthError> {
        let _timeout = tokio::time::Duration::from_secs(timeout_secs);
        let mut sleep_dur = tokio::time::Duration::from_millis(100);

        loop {
            // Check if response is already available
            let owned_response = {
                let responses = self.responses.lock().await;
                responses.get(&nonce).cloned()
            };
            if let Some(response) = owned_response {
                return self.handle_response(response).await;
            }

            // Check if we've exceeded the TTL
            {
                let pending = self.pending.lock().await;
                if let Some(request) = pending.get(&nonce) {
                    let elapsed = chrono::Utc::now()
                        .signed_duration_since(request.timestamp)
                        .num_seconds() as u64;
                    if elapsed > request.ttl {
                        return Err(AuthError::Timeout);
                    }
                }
                drop(pending);
            }

            // Wait for notification or timeout
            let notified =
                tokio::time::timeout(sleep_dur, self.response_notify.notified()).await;
            if notified.is_err() {
                // Timeout — check once more then return
                let owned_response = {
                    let responses = self.responses.lock().await;
                    responses.get(&nonce).cloned()
                };
                if let Some(response) = owned_response {
                    return self.handle_response(response).await;
                }
                return Err(AuthError::Timeout);
            }

            // Exponential backoff for polling (cap at 1s)
            sleep_dur = (sleep_dur * 2).min(tokio::time::Duration::from_secs(1));
        }
    }

    /// Clean up expired requests
    pub async fn cleanup(&self) {
        let mut pending = self.pending.lock().await;
        let now = chrono::Utc::now();
        pending.retain(|_, req| {
            let elapsed = now.signed_duration_since(req.timestamp).num_seconds();
            elapsed < req.ttl as i64
        });
    }
}

/// Verify an auth response signature using the device's pairing key.
///
/// In production, this uses the device's stored P-256 public key to verify
/// the HMAC-SHA256 signature. The signature covers:
///   HMAC-SHA256(pairing_key, nonce || username || phone_timestamp)
pub fn verify_signature(
    device: &PairedDevice,
    response: &AuthResponse,
) -> Result<(), String> {
    // In production:
    // 1. Decode the device's public key (P-256 uncompressed point)
    // 2. Decode the signature (HMAC-SHA256 output, hex-encoded)
    // 3. Compute HMAC-SHA256 over the message with the device's key
    // 4. Compare signatures

    // For now, placeholder — actual verification requires the pairing key
    // to be loaded as a k256::ecdsa::VerifyingKey
    let _ = (&device, response);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handler_creation() {
        let handler = AuthRequestHandler::new(PairingStore::new());
        assert!(handler.create_and_broadcast("testuser".to_string(), 30).await.is_err());
        // No paired devices, should fail
    }
}
