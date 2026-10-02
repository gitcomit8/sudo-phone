//! PAM integration for phone-based authentication.
//!
//! Two integration modes:
//!
//! 1. **pam_exec** (primary): A PAM module (`pam_exec.so`) calls a helper
//!    script that communicates with the daemon via a Unix socket.
//!    This is the recommended approach — it's simple and well-understood.
//!
//! 2. **pam_rust** (experimental): Direct PAM API integration using the
//!    `pam_rust` crate. This gives more control but is more complex.
//!
//! PAM flow:
//! 1. User runs `sudo`
//! 2. PAM calls `pam_sm_authenticate()`
//! 3. This module sends an auth request to the daemon
//! 4. Daemon broadcasts to phone, phone shows biometric prompt
//! 5. Phone responds with signed token
//! 6. Daemon verifies and returns success/failure
//! 7. PAM accepts or rejects

use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use tracing::{info, warn, error};

use crate::auth_request::{AuthRequestHandler, AuthResult};

#[derive(Error, Debug)]
pub enum PamError {
    #[error("Daemon not responding")]
    DaemonNotResponding,

    #[error("Auth failed: {0}")]
    AuthFailed(String),

    #[error("PAM error: {0}")]
    PamError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Unix socket path for daemon communication
pub const DAEMON_SOCKET: &str = "/run/phone-auth/daemon.sock";

/// Perform PAM authentication by communicating with the daemon.
///
/// This is called from the PAM module (via pam_exec) or directly
/// from the daemon's own PAM integration.
///
/// Arguments (via stdin or env):
/// - PAM_USER: the username requesting auth
/// - PAM_RUSER: the remote user (if applicable)
///
/// Returns PAM_SUCCESS (0) on success, PAM_AUTH_ERR (9) on failure.
pub async fn pam_authenticate(username: &str) -> Result<(), PamError> {
    info!("PAM authenticate for user: {}", username);

    // Connect to the daemon via Unix socket
    let socket_path = Path::new(DAEMON_SOCKET);
    if !socket_path.exists() {
        warn!("Daemon socket not found at {}", DAEMON_SOCKET);
        // Fallback: try pam_exec approach
        return pam_auth_via_exec(username).await;
    }

    // In production, this would:
    // 1. Connect to the daemon via a Unix domain socket
    // 2. Send a JSON auth request: {"username": "...", "action": "auth"}
    // 3. Wait for the response (with timeout)
    // 4. Return PAM_SUCCESS or PAM_AUTH_ERR

    // For now, use the in-process auth handler
    // (The daemon would create this when it starts)
    let handler = AuthRequestHandler::new(crate::pairing::PairingStore::new());

    // Create auth request
    let nonce = handler
        .create_and_broadcast(username.to_string(), 30)
        .await
        .map_err(|e| PamError::AuthFailed(e.to_string()))?;

    // Wait for response (with timeout)
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        handler.wait_for_response(nonce, 30)
    ).await;

    match result {
        Ok(Ok(AuthResult::Accepted)) => {
            info!("PAM auth accepted for user: {}", username);
            Ok(())
        }
        Ok(Ok(AuthResult::Rejected(reason))) => {
            warn!("PAM auth rejected for {}: {}", username, reason);
            Err(PamError::AuthFailed(reason))
        }
        Ok(Ok(AuthResult::Timeout)) => {
            warn!("PAM auth timeout for {}", username);
            Err(PamError::DaemonNotResponding)
        }
        Ok(Ok(AuthResult::ReplayAttack)) => {
            error!("PAM auth: replay attack detected for {}", username);
            Err(PamError::AuthFailed("Replay attack detected".to_string()))
        }
        Ok(Err(e)) => {
            warn!("PAM auth handler error for {}: {}", username, e);
            Err(PamError::AuthFailed(e.to_string()))
        }
        Err(_) => {
            warn!("PAM auth timeout waiting for response for {}", username);
            Err(PamError::DaemonNotResponding)
        }
    }
}

/// Fallback: authenticate via pam_exec helper script.
///
/// This is called when the daemon socket is not available.
/// It invokes a helper script that communicates with the daemon.
async fn pam_auth_via_exec(username: &str) -> Result<(), PamError> {
    // The helper script is installed at /usr/local/bin/phone-auth-helper
    // It communicates with the daemon and returns 0 on success, 1 on failure.
    //
    // PAM configuration (sudoers):
    //   auth sufficient /usr/lib/security/pam_exec.so expose_authtok /usr/local/bin/phone-auth-helper
    //
    // The helper script receives the username via PAM_AUTHTOK or stdin.

    let helper = "/usr/local/bin/phone-auth-helper";
    if !std::path::Path::new(helper).exists() {
        return Err(PamError::DaemonNotResponding);
    }

    // Run the helper with the username, wrapped in a timeout
    let output = tokio::time::timeout(
        Duration::from_secs(35),
        tokio::process::Command::new(helper)
            .arg(username)
            .output()
    ).await;

    let output = match output {
        Ok(Ok(output)) => output,
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(PamError::DaemonNotResponding);
        }
        Ok(Err(e)) => return Err(PamError::Io(e)),
        Err(_) => return Err(PamError::DaemonNotResponding),
    };

    if output.status.success() {
        info!("phone-auth-helper succeeded for {}", username);
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            "phone-auth-helper failed for {}: {}",
            username, stderr
        );
        Err(PamError::AuthFailed("Helper returned failure".to_string()))
    }
}

/// Generate the PAM configuration snippet for sudo.
///
/// This should be added to /etc/pam.d/sudo:
///
///   # Phone auth — biometric verification via paired phone
///   # Must be placed BEFORE the existing auth required line
///   auth sufficient /usr/lib/security/pam_exec.so quiet /usr/local/bin/phone-auth-pam-helper
///   auth required pam_deny.so  # fallback (or remove for phone-only)
///
/// And optionally enable pam_timestamp for cached credentials:
///
///   auth [success=1 default=ignore] pam_timestamp.so
///   auth sufficient pam_exec.so quiet /usr/local/bin/phone-auth-pam-helper
///   auth required pam_timestamp.so
pub fn pam_config_snippet() -> &'static str {
    r#"# Phone-based biometric authentication
# Place BEFORE other auth lines in /etc/pam.d/sudo
#
# auth sufficient /usr/lib/security/pam_exec.so expose_authtok /usr/local/bin/phone-auth-pam-helper
# auth required pam_deny.so
"#
}

/// Generate the sudoers configuration for phone auth.
///
/// This should be added to /etc/sudoers.d/phone-auth:
///
///   # Allow phone-auth to override sudo password requirement
///   # When pam_timestamp is enabled, successful phone auth caches for 5 min
///   Defaults timestamp_timeout=5
pub fn sudoers_config_snippet() -> &'static str {
    r#"# Phone auth sudoers configuration
# Install to: /etc/sudoers.d/phone-auth
#
# Cache successful phone auth for 5 minutes
Defaults timestamp_timeout=5
Defaults timestampdir="/run/sudo/ts"
"#
}

/// Install PAM configuration for phone auth.
pub fn install_pam_config(_config_dir: &Path) -> std::io::Result<()> {

    // Create PAM helper script
    let helper_path = Path::new("/usr/local/bin/phone-auth-pam-helper");
    let helper_script = r#"#!/bin/bash
# PAM helper script for phone-auth daemon
# Called by pam_exec.so during sudo authentication
#
# Usage: phone-auth-pam-helper
# Exits 0 on success (phone verified), 1 on failure

set -euo pipefail

# Get username from PAM_RUSER or whoami
USERNAME="${PAM_RUSER:-$(whoami)}"

# Connect to daemon and request auth
# The daemon listens on a Unix socket
RESPONSE=$(curl -s --unix-socket /run/phone-auth/daemon.sock \
    -d "{\"username\": \"$USERNAME\", \"action\": \"auth\"}" \
    --connect-timeout 30 --max-time 35 \
    unix:/run/phone-auth/daemon.sock/auth 2>/dev/null || echo "error")

if [ "$RESPONSE" = "accepted" ]; then
    exit 0
else
    exit 1
fi
"#;

    std::fs::write(helper_path, helper_script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(helper_path, std::fs::Permissions::from_mode(0o755))?;
    }

    info!("Installed PAM helper script to {}", helper_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pam_config_snippet() {
        let snippet = pam_config_snippet();
        assert!(snippet.contains("pam_exec"));
        assert!(snippet.contains("phone-auth"));
    }

    #[test]
    fn test_sudoers_config_snippet() {
        let snippet = sudoers_config_snippet();
        assert!(snippet.contains("timestamp_timeout"));
        assert!(snippet.contains("phone-auth"));
    }
}
