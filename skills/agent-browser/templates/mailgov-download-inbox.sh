#!/usr/bin/env bash
# mailgov-download-inbox.sh
# Downloads all inbox emails as .eml files from mail.mgovcloud.in (NICeMail / Zoho Mail)
#
# Prerequisites:
#   - Saved session at ./data/mailgov-auth.json
#   - agent-browser installed and on PATH
#
# Usage:
#   chmod +x skills/agent-browser/templates/mailgov-download-inbox.sh
#   ./skills/agent-browser/templates/mailgov-download-inbox.sh
#
# Optional env vars:
#   MAILGOV_STATE   path to auth state file   (default: ./data/mailgov-auth.json)
#   MAILGOV_OUT     output dir for .eml files  (default: ./data/artifacts/emails)
#   MAILGOV_LIMIT   max emails to download     (default: 0 = no limit)

set -euo pipefail

# --- config -----------------------------------------------------------------
STATE="${MAILGOV_STATE:-./data/mailgov-auth.json}"
OUT_DIR="${MAILGOV_OUT:-./data/artifacts/emails}"
LIMIT="${MAILGOV_LIMIT:-0}"
MANIFEST="${OUT_DIR}/manifest.jsonl"
LOG="${OUT_DIR}/download.log"
# macOS playwright temp root
PLAYWRIGHT_TMP="/var/folders"

# --- helpers ----------------------------------------------------------------
log() { echo "[$(date '+%H:%M:%S')] $*" | tee -a "$LOG"; }
die() { log "ERROR: $*"; exit 1; }

# Run agent-browser eval and decode the result.
# agent-browser eval returns JSON-encoded strings, e.g. "\"hello\"" for 'hello'
# and "\"[{...}]\"" for JSON.stringify(array). This helper strips one layer.
ab_eval_raw() {
    local script="$1"
    local raw
    raw=$(echo "$script" | agent-browser eval --stdin 2>/dev/null || true)
    # If result starts/ends with quotes (JSON string encoding), strip them
    python3 -c "
import sys, json
s = sys.stdin.read().strip()
try:
    decoded = json.loads(s)
    if isinstance(decoded, str):
        print(decoded)
    else:
        print(s)
except Exception:
    print(s)
" <<< "$raw"
}

slugify() {
    echo "$1" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]/-/g' | sed 's/-\+/-/g' | cut -c1-60
}

# Download one email by its Zoho message ID
# $1=msg_id  $2=aria-label  $3=padded seq
download_email() {
    local msg_id="$1"
    local label="$2"
    local seq="$3"

    local subject
    subject=$(echo "$label" | grep -oP '(?<=Subject ).*?(?=, Received time)' 2>/dev/null || echo "unknown")
    local slug
    slug=$(slugify "$subject")
    local dest="${OUT_DIR}/${seq}_${slug}.eml"

    if [[ -f "$dest" ]]; then
        log "SKIP $seq already exists"
        return 0
    fi

    log "DOWNLOAD $seq  id=$msg_id  $(echo "$subject" | cut -c1-70)"

    # 1. Click the email row in the list
    agent-browser eval "document.querySelector('[id=\"${msg_id}\"]').click(); 'ok'" > /dev/null 2>&1
    agent-browser wait 1500 > /dev/null 2>&1

    # 2. Find and click "Show more actions" button in the reading pane
    #    It is the LAST button with that aria-label (the one inside the reading pane, not the list toolbar)
    local more_ref
    more_ref=$(agent-browser snapshot -i 2>/dev/null \
        | grep 'Show more actions' | tail -1 | grep -oP '@e\d+' | head -1 || true)
    if [[ -z "$more_ref" ]]; then
        log "WARN $seq: 'Show more actions' not found — skipping"
        return 1
    fi
    agent-browser click "$more_ref" > /dev/null 2>&1
    agent-browser wait 400 > /dev/null 2>&1

    # 3. Click "Save as" (aria-label: "Choose to save email as")
    local save_ref
    save_ref=$(agent-browser snapshot -i 2>/dev/null \
        | grep 'Choose to save email as' | grep -oP '@e\d+' | head -1 || true)
    if [[ -z "$save_ref" ]]; then
        log "WARN $seq: Save-as not found — skipping"
        agent-browser press Escape > /dev/null 2>&1 || true
        return 1
    fi
    agent-browser click "$save_ref" > /dev/null 2>&1
    agent-browser wait 300 > /dev/null 2>&1

    # 4. Click ".eml file" from the sub-menu
    local eml_ref
    eml_ref=$(agent-browser snapshot -i 2>/dev/null \
        | grep '\.eml file' | grep -oP '@e\d+' | head -1 || true)
    if [[ -z "$eml_ref" ]]; then
        log "WARN $seq: EML option not found — skipping"
        agent-browser press Escape > /dev/null 2>&1 || true
        agent-browser press Escape > /dev/null 2>&1 || true
        return 1
    fi

    # Timestamp marker so we can find the new download file
    local ts_marker="/tmp/mailgov_ts_${seq}_$$"
    touch "$ts_marker"

    agent-browser click "$eml_ref" > /dev/null 2>&1
    agent-browser wait 1000 > /dev/null 2>&1

    # 5. Locate the file Playwright downloaded to its temp dir
    local found_file=""
    for _ in $(seq 1 20); do
        found_file=$(find "$PLAYWRIGHT_TMP" -newer "$ts_marker" -type f 2>/dev/null \
            | grep "playwright-artifacts" | head -1 || true)
        if [[ -n "$found_file" ]] && \
           head -3 "$found_file" 2>/dev/null | grep -qiE "^(Return-Path|Received|From|MIME-Version|Date|X-Mailer):"; then
            break
        fi
        found_file=""
        sleep 0.5
    done
    rm -f "$ts_marker"

    if [[ -z "$found_file" ]]; then
        log "WARN $seq: Download not detected for msg_id=$msg_id"
        return 1
    fi

    cp "$found_file" "$dest"
    local size
    size=$(wc -c < "$dest")
    log "  -> saved ${size}B to $dest"

    # Append to manifest (JSONL)
    printf '{"seq":%s,"msg_id":"%s","file":"%s","label":"%s"}\n' \
        "${seq#0}" "$msg_id" "$dest" \
        "$(echo "$label" | sed 's/"/\\"/g')" \
        >> "$MANIFEST"

    return 0
}

# --- main -------------------------------------------------------------------
main() {
    [[ -f "$STATE" ]] || die "Session file not found: $STATE  (run mailgov-login.sh first)"
    mkdir -p "$OUT_DIR"
    : > "$LOG"  # reset log
    log "=== mailgov-download-inbox start ==="
    log "Output: $OUT_DIR  |  Limit: ${LIMIT:-none}"

    # Fresh browser with saved session
    agent-browser close 2>/dev/null || true
    sleep 1
    agent-browser state load "$STATE"
    agent-browser open "https://mail.mgovcloud.in/zm/#mail/folder/inbox"
    # Wait for email list to render (Zoho is slow to hydrate)
    agent-browser wait 5000 > /dev/null 2>&1
    agent-browser wait "[role='listbox']" > /dev/null 2>&1

    local url
    url=$(agent-browser get url 2>/dev/null || true)
    if ! echo "$url" | grep -q "mail.mgovcloud.in"; then
        die "Not authenticated — session expired. Re-login and save state."
    fi
    log "Inbox loaded: $url"

    local seq=1
    local downloaded=0
    local failed=0
    local seen_ids_file="/tmp/mailgov_seen_$$"
    touch "$seen_ids_file"

    while true; do
        # Get all visible email IDs
        local emails_json
        emails_json=$(ab_eval_raw '
            JSON.stringify(
                Array.from(document.querySelectorAll("[role=\"option\"][id]")).map(el => ({
                    id: el.id,
                    label: el.getAttribute("aria-label") || ""
                }))
            )
        ')

        local count
        count=$(python3 -c "import sys,json; data=json.loads(sys.stdin.read()); print(len(data))" <<< "$emails_json" 2>/dev/null || echo 0)

        if [[ "$count" -eq 0 ]]; then
            log "No email rows found. Done."
            break
        fi
        log "Visible email rows: $count  (downloaded so far: $downloaded)"

        # Track whether we found any new emails on this scroll position
        local new_on_this_scroll=0

        for i in $(seq 0 $((count - 1))); do
            local msg_id label
            msg_id=$(python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(d[$i]['id'])" <<< "$emails_json" 2>/dev/null || true)
            label=$(python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(d[$i]['label'])" <<< "$emails_json" 2>/dev/null || true)

            [[ -z "$msg_id" ]] && continue
            grep -qF "$msg_id" "$seen_ids_file" && continue  # already processed

            echo "$msg_id" >> "$seen_ids_file"
            ((new_on_this_scroll++))

            local padded_seq
            padded_seq=$(printf '%05d' "$seq")

            if download_email "$msg_id" "$label" "$padded_seq"; then
                ((downloaded++))
            else
                ((failed++))
            fi

            ((seq++))

            if [[ "$LIMIT" -gt 0 && "$downloaded" -ge "$LIMIT" ]]; then
                log "Reached limit of $LIMIT."
                break 2
            fi

            # Brief pause between emails to be a good citizen
            sleep 0.3
        done

        # If no new emails on this scroll, we've processed everything visible
        if [[ "$new_on_this_scroll" -eq 0 ]]; then
            log "No new emails after scroll. All done."
            break
        fi

        # Scroll down to reveal more emails
        log "Scrolling for more emails..."
        agent-browser eval '
            const list = document.querySelector("[role=\"listbox\"]");
            if (list) list.scrollTop = list.scrollHeight;
            "ok"
        ' > /dev/null 2>&1
        agent-browser wait 2000 > /dev/null 2>&1
    done

    log ""
    log "=== Summary ==="
    log "Downloaded : $downloaded"
    log "Failed     : $failed"
    log "Manifest   : $MANIFEST"
    log "Log        : $LOG"

    rm -f "$seen_ids_file"
    agent-browser close 2>/dev/null || true
}

main "$@"
