#!/usr/bin/env python3
"""
skill: mailgov-download-inbox
description: Download all inbox emails as .eml files from mail.gov.in (NICeMail / Zoho Mail)
triggers: download emails, sync inbox, download mail, backup email, export inbox, download government mail
requires: MAILGOV_STATE
optional: MAILGOV_OUT, MAILGOV_LIMIT
category: email

mailgov-download-inbox.py
Downloads all inbox emails as .eml files from mail.mgovcloud.in (NICeMail / Zoho Mail).

Prerequisites:
  - Saved session at ./data/mailgov-auth.json  (run mailgov-login.sh first)
  - `agent-browser` installed and on PATH

Usage:
  python3 skills/agent-browser/templates/mailgov-download-inbox.py

Env vars:
  MAILGOV_STATE   path to auth state file   (default: ./data/mailgov-auth.json)
  MAILGOV_OUT     output dir for .eml files  (default: ./data/artifacts/emails)
  MAILGOV_LIMIT   max emails to download, 0=unlimited (default: 0)
"""

import glob
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from datetime import datetime
from pathlib import Path

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
STATE   = os.environ.get("MAILGOV_STATE", "./data/mailgov-auth.json")
OUT_DIR = Path(os.environ.get("MAILGOV_OUT",  "./data/artifacts/emails"))
LIMIT   = int(os.environ.get("MAILGOV_LIMIT", "0"))
PLAYWRIGHT_TMP = "/var/folders"  # macOS temp root

MANIFEST = OUT_DIR / "manifest.jsonl"
LOG_FILE = OUT_DIR / "download.log"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
def log(msg: str) -> None:
    line = f"[{datetime.now():%H:%M:%S}] {msg}"
    print(line)
    with open(LOG_FILE, "a") as f:
        f.write(line + "\n")


def ab(*args, capture=False, ignore_errors=False) -> str:
    """Run agent-browser with given args. Returns stdout."""
    cmd = ["agent-browser"] + list(args)
    result = subprocess.run(cmd, capture_output=capture, text=True)
    if not ignore_errors and result.returncode not in (0, 1):
        raise RuntimeError(f"agent-browser {' '.join(args)} failed: {result.stderr}")
    return (result.stdout or "").strip()


def ab_eval(js: str) -> str:
    """Run agent-browser eval, unwrap the JSON-encoded string result."""
    proc = subprocess.run(
        ["agent-browser", "eval", "--stdin"],
        input=js, capture_output=True, text=True
    )
    raw = (proc.stdout or "").strip()
    # agent-browser wraps string results in JSON quotes: "\"value\""
    try:
        decoded = json.loads(raw)
        return decoded if isinstance(decoded, str) else raw
    except Exception:
        return raw


def ab_snapshot() -> str:
    """Return the interactive snapshot text."""
    proc = subprocess.run(
        ["agent-browser", "snapshot", "-i"],
        capture_output=True, text=True
    )
    return proc.stdout or ""


def extract_ref(snapshot: str, pattern: str) -> str:
    """Find first @eN ref on a line matching the given regex pattern."""
    for line in snapshot.splitlines():
        if re.search(pattern, line, re.IGNORECASE):
            m = re.search(r"\[ref=(e\d+)\]", line)
            if m:
                return "@" + m.group(1)
    return ""


def extract_ref_last(snapshot: str, pattern: str) -> str:
    """Find LAST @eN ref on a line matching the given regex pattern."""
    found = ""
    for line in snapshot.splitlines():
        if re.search(pattern, line, re.IGNORECASE):
            m = re.search(r"\[ref=(e\d+)\]", line)
            if m:
                found = "@" + m.group(1)
    return found


def get_email_refs_and_ids(snapshot: str) -> list[tuple[str, str, str]]:
    """
    Parse snapshot to get (ref, label) for each email option row,
    and fetch their IDs from the DOM in the same order.
    Returns list of (ref, msg_id, label).
    """
    # Collect refs and labels for option rows from snapshot
    options = []
    for line in snapshot.splitlines():
        if re.search(r'^\s*- option\s+"', line):
            m_ref = re.search(r"\[ref=(e\d+)\]", line)
            # Label: text between first pair of double-quotes
            m_lbl = re.search(r'"([^"]+)"', line)
            if m_ref and m_lbl:
                options.append(("@" + m_ref.group(1), m_lbl.group(1)))

    if not options:
        return []

    # Fetch all email IDs from DOM in document order
    raw = ab_eval("""
        JSON.stringify(
            Array.from(document.querySelectorAll('[role="option"][id]')).map(el => el.id)
        )
    """)
    try:
        ids = json.loads(raw)
    except Exception:
        ids = []

    # Match by position (both lists are in document/visual order)
    result = []
    for i, (ref, label) in enumerate(options):
        msg_id = ids[i] if i < len(ids) else ""
        result.append((ref, msg_id, label))
    return result


def slugify(text: str) -> str:
    text = text.lower()
    text = re.sub(r"[^a-z0-9]+", "-", text)
    text = text.strip("-")
    return text[:60]


def subject_from_label(label: str) -> str:
    m = re.search(r"Subject (.+?), Received time", label)
    return m.group(1) if m else "unknown"


def find_new_eml(since_path: str) -> str:
    """Find a file in playwright temp dir newer than since_path that looks like EML."""
    for _ in range(20):
        matches = glob.glob(f"{PLAYWRIGHT_TMP}/**/playwright-artifacts*/**", recursive=True)
        for p in matches:
            if not os.path.isfile(p):
                continue
            if os.path.getmtime(p) <= os.path.getmtime(since_path):
                continue
            try:
                with open(p, "rb") as f:
                    head = f.read(512).decode("utf-8", errors="ignore")
                if re.match(r"^(Return-Path|Received|From|MIME-Version|Date|X-Mailer):", head):
                    return p
            except Exception:
                pass
        time.sleep(0.5)
    return ""


# ---------------------------------------------------------------------------
# Download one email
# ---------------------------------------------------------------------------
def download_email(row_ref: str, msg_id: str, label: str, seq: int) -> bool:
    subject = subject_from_label(label)
    slug = slugify(subject)
    dest = OUT_DIR / f"{seq:05d}_{slug}.eml"

    if dest.exists():
        log(f"SKIP {seq:05d} already exists: {dest.name}")
        return True

    log(f"DOWNLOAD {seq:05d}  id={msg_id}  {subject[:70]}")

    # 1. Use a real Playwright click on the email row ref (triggers full event chain)
    result = subprocess.run(
        ["agent-browser", "click", row_ref],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        log(f"  WARN: Click on {row_ref} failed: {result.stderr.strip()[:80]}")
        return False
    time.sleep(2.0)  # give reading pane time to render

    # 2. Find "Show more actions" in reading pane (LAST occurrence = reading pane, not toolbar)
    snap = ab_snapshot()
    more_ref = extract_ref_last(snap, r"Show more actions")
    if not more_ref:
        log(f"  WARN: 'Show more actions' not found — retrying after extra wait")
        time.sleep(2.0)
        snap = ab_snapshot()
        more_ref = extract_ref_last(snap, r"Show more actions")
    if not more_ref:
        log(f"  WARN: Giving up on {seq:05d} — reading pane did not open")
        return False

    ab("click", more_ref, ignore_errors=True)
    time.sleep(0.4)

    # 3. Click "Save as" menu item
    snap = ab_snapshot()
    save_ref = extract_ref(snap, r"Choose to save email as|Save as")
    if not save_ref:
        log(f"  WARN: Save-as not in menu for {seq:05d}")
        ab("press", "Escape", ignore_errors=True)
        return False

    ab("click", save_ref, ignore_errors=True)
    time.sleep(0.3)

    # 4. Click ".eml file" sub-menu
    snap = ab_snapshot()
    eml_ref = extract_ref(snap, r"\.eml file")
    if not eml_ref:
        log(f"  WARN: EML option not in sub-menu for {seq:05d}")
        ab("press", "Escape", ignore_errors=True)
        ab("press", "Escape", ignore_errors=True)
        return False

    # Timestamp marker before triggering download
    with tempfile.NamedTemporaryFile(delete=False, suffix=".ts") as ts:
        ts_path = ts.name

    ab("click", eml_ref, ignore_errors=True)
    time.sleep(0.8)

    # 5. Find the downloaded file and move it
    found = find_new_eml(ts_path)
    os.unlink(ts_path)

    if not found:
        log(f"  WARN: Download file not detected for {seq:05d} id={msg_id}")
        return False

    import shutil
    shutil.copy2(found, dest)
    size = dest.stat().st_size
    log(f"  -> saved {size}B to {dest.name}")

    with open(MANIFEST, "a") as mf:
        mf.write(json.dumps({
            "seq": seq,
            "msg_id": msg_id,
            "file": str(dest),
            "label": label,
        }) + "\n")

    return True


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main():
    if not Path(STATE).exists():
        print(f"ERROR: Session file not found: {STATE}")
        print("       Run mailgov-login.sh first to save a session.")
        sys.exit(1)

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    LOG_FILE.write_text("")  # reset log

    log("=== mailgov-download-inbox start ===")
    log(f"Output: {OUT_DIR}  |  Limit: {LIMIT or 'none'}")

    # Start fresh browser with saved session
    ab("close", ignore_errors=True)
    time.sleep(1)
    ab("state", "load", STATE)
    ab("open", "https://mail.mgovcloud.in/zm/#mail/folder/inbox")
    time.sleep(5)  # Zoho is slow to hydrate
    ab("wait", "[role='listbox']", ignore_errors=True)

    url = ab("get", "url", capture=True)
    if "mail.mgovcloud.in" not in url:
        log("ERROR: Not authenticated — session expired. Re-login and save state.")
        sys.exit(1)
    log(f"Inbox loaded: {url}")

    seq = 1
    downloaded = 0
    failed = 0
    seen_ids: set[str] = set()

    while True:
        # Get snapshot to extract refs alongside IDs
        snap = ab_snapshot()
        emails = get_email_refs_and_ids(snap)

        if not emails:
            log("No email rows found. Done.")
            break

        log(f"Visible email rows: {len(emails)}  (downloaded so far: {downloaded})")

        new_on_scroll = 0
        for (row_ref, msg_id, label) in emails:
            if not msg_id or msg_id in seen_ids:
                continue

            seen_ids.add(msg_id)
            new_on_scroll += 1

            ok = download_email(row_ref, msg_id, label, seq)
            if ok:
                downloaded += 1
            else:
                failed += 1
            seq += 1

            if LIMIT and downloaded >= LIMIT:
                log(f"Reached limit of {LIMIT}.")
                break

        if LIMIT and downloaded >= LIMIT:
            break

        if new_on_scroll == 0:
            log("No new emails after scroll. All done.")
            break

        # Scroll the listbox to reveal more emails
        log("Scrolling for more emails...")
        ab_eval("""
            const list = document.querySelector('[role="listbox"]');
            if (list) list.scrollTop = list.scrollHeight;
            'ok'
        """)
        time.sleep(2.5)

    log("")
    log("=== Summary ===")
    log(f"Downloaded : {downloaded}")
    log(f"Failed     : {failed}")
    log(f"Manifest   : {MANIFEST}")
    log(f"Log        : {LOG_FILE}")

    ab("close", ignore_errors=True)


if __name__ == "__main__":
    main()
