#!/bin/bash
# setup-pam.sh — Configure PAM and sudoers for phone-auth
#
# Usage: sudo ./setup-pam.sh
#
# This script:
# 1. Installs the PAM helper script
# 2. Modifies /etc/pam.d/sudo to include phone auth
# 3. Creates sudoers drop-in for timestamp caching

set -euo pipefail

if [ "$EUID" -ne 0 ]; then
    echo "Error: this script must be run as root (use sudo)"
    exit 1
fi

PAM_HELPER="/usr/local/bin/phone-auth-pam-helper"
PAM_CONF="/etc/pam.d/sudo"
SUDOERS_CONF="/etc/sudoers.d/phone-auth"
BACKUP_DIR="/etc/phone-auth/backups"

echo "=== Phone Auth PAM Setup ==="

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Backup original PAM config
if [ -f "$PAM_CONF" ]; then
    cp "$PAM_CONF" "$BACKUP_DIR/sudo.bak.$(date +%Y%m%d%H%M%S)"
    echo "[✓] Backed up $PAM_CONF"
fi

# Install PAM helper script
cat > "$PAM_HELPER" << 'HELPER'
#!/bin/bash
# PAM helper for phone-auth daemon
# Called by pam_exec.so during sudo authentication
# Exits 0 on success (phone verified), 1 on failure

set -uo pipefail

USERNAME="${PAM_USER:-$(whoami)}"

# Try Unix socket first, fall back to HTTP
if [ -S /run/phone-auth/daemon.sock ]; then
    RESPONSE=$(curl -s --unix-socket /run/phone-auth/daemon.sock \
        -d "{\"username\": \"$USERNAME\", \"action\": \"auth\"}" \
        --connect-timeout 30 --max-time 35 \
        unix:/run/phone-auth/daemon.sock/auth 2>/dev/null || echo "error")
else
    RESPONSE="error"
fi

if [ "$RESPONSE" = "accepted" ]; then
    exit 0
else
    exit 1
fi
HELPER

chmod 755 "$PAM_HELPER"
echo "[✓] Installed PAM helper to $PAM_HELPER"

# Modify /etc/pam.d/sudo
# Insert phone auth line BEFORE the first 'auth required' or 'auth sufficient' line
if grep -q "phone-auth" "$PAM_CONF" 2>/dev/null; then
    echo "[✓] phone-auth already configured in $PAM_CONF"
else
    # Insert after the first auth line, or at the top if no auth lines exist
    if grep -q "^auth " "$PAM_CONF"; then
        sed -i '/^auth /i # Phone auth — biometric verification via paired phone\nauth sufficient /usr/lib/security/pam_exec.so quiet '"$PAM_HELPER"'' "$PAM_CONF"
    else
        echo "# Phone auth — biometric verification via paired phone" >> "$PAM_CONF"
        echo "auth sufficient /usr/lib/security/pam_exec.so quiet $PAM_HELPER" >> "$PAM_CONF"
    fi
    echo "[✓] Added phone-auth to $PAM_CONF"
fi

# Create sudoers drop-in
cat > "$SUDOERS_CONF" << 'SUDOERS'
# Phone auth sudoers configuration
# Cache successful phone auth for 5 minutes
Defaults timestamp_timeout=5
Defaults timestampdir="/run/sudo/ts"
SUDOERS

chmod 440 "$SUDOERS_CONF"
visudo -c -f "$SUDOERS_CONF"
echo "[✓] Installed sudoers config to $SUDOERS_CONF"

echo ""
echo "=== Setup complete ==="
echo "Next steps:"
echo "  1. Build and install the daemon: cargo build --release && sudo cp target/release/phone-auth-daemon /usr/local/bin/"
echo "  2. Create config dir: sudo mkdir -p /etc/phone-auth && sudo chmod 700 /etc/phone-auth"
echo "  3. Pair with a phone: phone-auth-daemon pair"
echo "  4. Start the daemon: sudo phone-auth-daemon start"
echo "  5. Test: sudo -u $(whoami) whoami"
