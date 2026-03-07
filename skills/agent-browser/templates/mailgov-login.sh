#!/bin/bash
# Multi-step login for mail.gov.in (NIC Accounts)
#
# Flow: Email → Next → Password → Submit → OTP (2FA)
#
# Usage:
#   export APP_USERNAME="your.email@gov.in"
#   export APP_PASSWORD="your_password"
#   ./mailgov-login.sh [--headed] [--pause] [state-file]
#
# Options:
#   --headed  Browser window visible (run from Terminal.app for display)
#   --pause   Pause after each step so you can see output before continuing
#
# State file: Save session after successful login for reuse

set -euo pipefail

LOGIN_URL="https://accounts.mgovcloud.in/signin?servicename=VirtualOffice&serviceurl=https%3A%2F%2Fmail.mgovcloud.in%2F"
STATE_FILE="./data/mailgov-auth.json"

HEADED=""
PAUSE=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --headed) HEADED="--headed"; shift ;;
        --pause)  PAUSE=1; shift ;;
        *)        STATE_FILE="$1"; shift ;;
    esac
done

pause_if_needed() {
    [[ -n "$PAUSE" ]] && read -p "Press Enter to continue..."
}

: "${APP_USERNAME:?Set APP_USERNAME (your gov.in email)}"
: "${APP_PASSWORD:?Set APP_PASSWORD}"

echo "=========================================="
echo "mail.gov.in multi-step login"
echo "=========================================="

# ---------------------------------------------------------------------------
# STEP 1: Open page, enter email, click Next
# ---------------------------------------------------------------------------
echo ""
echo ">>> STEP 1: Enter email and click Next"
echo "--------------------------------------"

agent-browser $HEADED open "$LOGIN_URL"
agent-browser wait --load networkidle

echo "Page structure (Step 1):"
agent-browser snapshot -i
echo ""
pause_if_needed

# Fill email (selector #login_id or ref from snapshot)
agent-browser fill "#login_id" "$APP_USERNAME"
echo "✓ Filled email"

# Click Next / Submit email (button "Submit email address")
agent-browser find text "Submit email address" click 2>/dev/null || agent-browser find role button click --name "Submit email address" 2>/dev/null || agent-browser click "button[type='submit']"
echo "✓ Clicked Next"
agent-browser wait 2000

echo ""
echo "After Step 1:"
agent-browser snapshot -i
echo ""
pause_if_needed

# ---------------------------------------------------------------------------
# STEP 2: Password appears — fill and submit
# ---------------------------------------------------------------------------
echo ""
echo ">>> STEP 2: Enter password and submit"
echo "--------------------------------------"

agent-browser wait "#password" --timeout 10000 2>/dev/null || agent-browser wait 3000
echo "Password step - page structure:"
agent-browser snapshot -i
echo ""
pause_if_needed

agent-browser fill "#password" "$APP_PASSWORD"
echo "✓ Filled password"

# Click Sign in / Submit
agent-browser find text "Sign in" click 2>/dev/null || \
agent-browser find role button click --name "Sign in" 2>/dev/null || \
agent-browser click "button[type='submit']" 2>/dev/null || \
agent-browser click "input[type='submit']"
echo "✓ Clicked Sign in"
agent-browser wait 3000

echo ""
echo "After Step 2:"
agent-browser snapshot -i
echo ""
pause_if_needed

# ---------------------------------------------------------------------------
# STEP 3: OTP (2FA) — wait for user input
# ---------------------------------------------------------------------------
echo ""
echo ">>> STEP 3: OTP / 2FA"
echo "--------------------------------------"

# Check if OTP field appeared
if agent-browser eval 'document.querySelector("#otp, #mfa_otp, #mfa_totp") !== null' 2>/dev/null | grep -q true; then
    echo "OTP field detected. Enter OTP when prompted."
    echo ""
    read -p "Enter OTP from your device: " OTP
    agent-browser fill "#otp" "$OTP" 2>/dev/null || agent-browser fill "#mfa_otp" "$OTP" 2>/dev/null || agent-browser fill "#mfa_totp" "$OTP"
    echo "✓ Filled OTP"
    agent-browser click "button[type='submit']" 2>/dev/null || agent-browser press Enter
    agent-browser wait 5000
fi

# Also handle case where OTP field appears later (dynamic)
echo "Waiting for login to complete (up to 2 min)..."
if agent-browser wait --url "**/mail**" --timeout 120000 2>/dev/null; then
    echo "✓ Login successful"
else
    echo "Checking current state..."
    agent-browser snapshot -i
    echo ""
    # Maybe OTP is needed
    if agent-browser eval 'document.querySelector("#otp, #mfa_otp, #mfa_totp") !== null' 2>/dev/null | grep -q true; then
        read -p "OTP field found. Enter OTP: " OTP
        agent-browser fill "#otp" "$OTP" 2>/dev/null || agent-browser fill "#mfa_otp" "$OTP" 2>/dev/null || agent-browser fill "#mfa_totp" "$OTP"
        agent-browser click "button[type='submit']" 2>/dev/null || agent-browser press Enter
        agent-browser wait --url "**/mail**" --timeout 60000
    fi
fi

echo ""
echo ">>> Final state:"
agent-browser get url
agent-browser snapshot -i
echo ""

# Save state for reuse
mkdir -p "$(dirname "$STATE_FILE")"
agent-browser state save "$STATE_FILE"
echo "✓ State saved to $STATE_FILE"

agent-browser close
echo ""
echo "Done. Next time: agent-browser state load $STATE_FILE && agent-browser open https://mail.mgovcloud.in"
