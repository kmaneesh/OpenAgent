# mail.gov.in Multi-Step Login Guide

NIC Accounts (mail.gov.in) uses a **3-step login flow**:

1. **Step 1:** Enter email → Click **Next** (Submit email address)
2. **Step 2:** Password field appears → Enter password → Click **Sign in**
3. **Step 3:** OTP/2FA field appears → Enter OTP from device → Submit

---

## Quick Start (Script)

```bash
# 1. Set credentials (use env vars — never paste in chat)
export APP_USERNAME="your.email@gov.in"
export APP_PASSWORD="your_password"

# 2. Run the script
cd /path/to/OpenAgent
chmod +x skills/agent-browser/templates/mailgov-login.sh
./skills/agent-browser/templates/mailgov-login.sh
```

### Options

| Flag | Purpose |
|------|---------|
| `--headed` | Show browser window (run from Terminal.app) |
| `--pause` | Pause after each step so you can see output |

```bash
# See the browser and pause at each step
./skills/agent-browser/templates/mailgov-login.sh --headed --pause
```

---

## Manual Step-by-Step (See Output at Each Step)

Run these commands **one at a time** in your terminal to see output after each step.

### Step 1: Open page, enter email, click Next

```bash
agent-browser --headed open "https://accounts.mgovcloud.in/signin?servicename=VirtualOffice&serviceurl=https%3A%2F%2Fmail.mgovcloud.in%2F"
agent-browser wait --load networkidle
agent-browser snapshot -i
```

**Output:** You'll see the form with email field (`@e36` or similar), "Submit email address" button (`@e37`).

```bash
agent-browser fill "#login_id" "your.email@gov.in"
agent-browser click "button:has-text('Submit email address')"
agent-browser wait 2000
agent-browser snapshot -i
```

**Output:** Password field should now be visible.

---

### Step 2: Enter password, submit

```bash
agent-browser fill "#password" "your_password"
agent-browser click "button[type='submit']"
agent-browser wait 3000
agent-browser snapshot -i
```

**Output:** Either OTP field appears, or you're redirected to mail.

---

### Step 3: OTP (2FA)

When you see an OTP/2FA field:

```bash
# When prompted, enter the OTP from your phone/app
read -p "Enter OTP: " OTP
agent-browser fill "#otp" "$OTP"
agent-browser press Enter
agent-browser wait --url "**/mail**" --timeout 60000
```

---

### Save session for reuse

```bash
agent-browser state save ./data/mailgov-auth.json
agent-browser close
```

---

## Reuse Saved Session (Skip Login)

```bash
agent-browser state load ./data/mailgov-auth.json
agent-browser open https://mail.mgovcloud.in
agent-browser snapshot -i
```

---

## Loading Saved Session for Claude / Cursor / AI Agents

Use this when you've already logged in, saved state, and want an AI assistant (Claude, Cursor, etc.) to continue from the mail inbox.

### Prerequisites

- Session saved at `./data/mailgov-auth.json` (or your project path)
- Run from the **OpenAgent project root** so the path resolves

### Instructions for You (Human)

1. **Save the session** (after logging in manually or via script):
   ```bash
   agent-browser state save ./data/mailgov-auth.json
   agent-browser close
   ```

2. **Tell the AI:** "Session saved at ./data/mailgov-auth.json. Load it and continue from mail.gov.in."

### Instructions for the AI (Claude / Cursor)

When the user says the session is saved, run:

```bash
# Close any existing browser first
agent-browser close 2>/dev/null || true

# Load saved state (must be before open)
agent-browser state load ./data/mailgov-auth.json

# Open mail (redirects to workplace with mail)
agent-browser open https://mail.mgovcloud.in

# Wait for load, then snapshot
agent-browser wait 5000
agent-browser snapshot -i
```

**Important:** `state load` must run **before** `open` and requires a **fresh browser** (close first if one is running).

### Paths

| Context | State file path |
|--------|-----------------|
| OpenAgent project | `./data/mailgov-auth.json` |
| Absolute | `/Users/<you>/Sites/kmaneesh.github/OpenAgent/data/mailgov-auth.json` |

### After Load

- **URL:** `https://workplace.mgovcloud.in/#mail_app/mail/folder/inbox`
- **Mail iframe:** `#mailIframe` (mail content may not snapshot; use Search @e10 from outer frame)
- **Session expires:** Re-login and save state again

---

## Element Selectors (mail.gov.in)

| Step | Field | Selector |
|------|-------|-----------|
| 1 | Email | `#login_id` |
| 1 | Next button | `button:has-text('Submit email address')` |
| 2 | Password | `#password` |
| 2 | Sign in | `button[type='submit']` |
| 3 | OTP | `#otp` or `#mfa_otp` or `#mfa_totp` |

---

## Troubleshooting

| Issue | Fix |
|-------|-----|
| No visible browser | Run from **Terminal.app** (not Cursor) with `--headed` |
| "Could not find username field" | Use the script or manual steps; auth vault doesn't support multi-step |
| OTP timeout | Use `--pause` and enter OTP when prompted |
| Session expired | Re-run full login; save state again |
