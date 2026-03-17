/// Input sanitization — credential scrubbing + prompt injection detection.
///
/// Applied inside the Guard layer before every request reaches STT or Cortex.
/// Fires on both paths:
///   - HTTP `POST /step`: `user_input` field scrubbed in the buffered request body.
///   - Dispatch loop: `content` scrubbed before `cortex.step` is called.
///
/// Two passes in one call to `process()`:
///
/// 1. **Credential scrubbing** — scans for `keyword[:=]value` patterns where
///    `keyword` is a known secret field name (token, api_key, password, …).
///    Values ≥ 8 chars are redacted to `<first-4-chars>[REDACTED]`.
///    Short values (< 8 chars) are left alone — too short to be real secrets.
///    A `WARN` log is emitted with `session_id`; the secret value is never logged.
///
/// 2. **Prompt injection detection** — scans for known manipulation phrases
///    ("ignore previous instructions", "you are now", …) case-insensitively.
///    Emits a `WARN` log but does NOT modify the text.
///
/// No external dependencies — hand-rolled byte scanning for minimal binary
/// footprint on Pi targets.

use tracing::warn;

/// Secret field keyword fragments — matched case-insensitively.
const CRED_KEYWORDS: &[&str] = &[
    "api-key", "api_key", "apikey",
    "password", "passwd",
    "credential",
    "bearer",
    "secret",
    "token",
    "auth",
];

/// Prompt injection phrases — matched case-insensitively.
const INJECTION_PHRASES: &[&str] = &[
    "ignore previous instructions",
    "ignore all previous",
    "disregard previous",
    "disregard the above",
    "forget your instructions",
    "do not follow",
    "override instructions",
    "you are now",
    "your new instructions",
    "new persona",
    "act as if",
    "pretend you are",
    "pretend to be",
    "roleplay as",
    "jailbreak",
    "dan mode",
];

/// Values shorter than this are not redacted (too short to be real secrets).
const MIN_SECRET_LEN: usize = 8;

/// Plaintext characters kept before `[REDACTED]`.
const VISIBLE_PREFIX: usize = 4;

/// Sanitize `input` — scrub credentials (mutates) and detect injection (warn only).
///
/// `context` is a human-readable label for logs (e.g. `"session:<id>"` or
/// `"platform:discord channel_id:123"`).
pub fn process(input: &str, context: &str) -> String {
    let cleaned = scrub_credentials(input, context);
    detect_injection(&cleaned, context);
    cleaned
}

// ---------------------------------------------------------------------------
// Credential scrubbing
// ---------------------------------------------------------------------------

fn scrub_credentials(input: &str, context: &str) -> String {
    let lower = input.to_lowercase();
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut pos = 0usize;

    while pos < input.len() {
        // Find earliest keyword match from current position.
        let found = CRED_KEYWORDS
            .iter()
            .filter_map(|kw| lower[pos..].find(kw).map(|off| (pos + off, kw.len())))
            .min_by_key(|(start, _)| *start);

        let (kw_start, kw_len) = match found {
            Some(f) => f,
            None => {
                out.push_str(&input[pos..]);
                break;
            }
        };

        out.push_str(&input[pos..kw_start]);
        pos = kw_start + kw_len;
        let after_kw = pos;

        // Skip optional whitespace after keyword.
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }

        // Require `=` or `:` separator — OR allow no separator for scheme-style
        // keywords like "bearer" where the value follows directly after whitespace
        // (e.g. `Authorization: bearer <token>`).
        let kw_text = &lower[kw_start..kw_start + kw_len];
        let separator_optional = kw_text == "bearer";

        if pos >= bytes.len() || (bytes[pos] != b'=' && bytes[pos] != b':') {
            if separator_optional && pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                // Value starts here (no separator consumed).
                out.push_str(&input[kw_start..pos]);
            } else {
                out.push_str(&input[kw_start..after_kw]);
                pos = after_kw;
                continue;
            }
        } else {
            out.push_str(&input[kw_start..=pos]);
            pos += 1; // consume separator

            // Skip optional whitespace after separator.
            while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
                pos += 1;
            }
        }

        // Collect value token (non-whitespace run).
        let val_start = pos;
        while pos < bytes.len()
            && bytes[pos] != b' '
            && bytes[pos] != b'\t'
            && bytes[pos] != b'\n'
            && bytes[pos] != b'\r'
        {
            pos += 1;
        }
        let value = &input[val_start..pos];

        if value.len() >= MIN_SECRET_LEN {
            warn!(context, "guard.scrub.credential_detected — redacting");
            let prefix_end = value.len().min(VISIBLE_PREFIX);
            out.push_str(&value[..prefix_end]);
            out.push_str("[REDACTED]");
        } else {
            out.push_str(value);
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Prompt injection detection
// ---------------------------------------------------------------------------

fn detect_injection(input: &str, context: &str) {
    let lower = input.to_lowercase();
    for phrase in INJECTION_PHRASES {
        if lower.contains(phrase) {
            warn!(context, phrase, "guard.scrub.injection_detected");
            return; // one warning per message
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_api_key_equals() {
        let out = scrub_credentials("api_key=sk-anth1234567890abcdef", "ctx");
        assert!(out.contains("[REDACTED]"), "expected redaction: {out}");
        assert!(out.starts_with("api_key=sk-a"), "prefix preserved: {out}");
        assert!(!out.contains("1234567890abcdef"), "secret leaked: {out}");
    }

    #[test]
    fn redacts_token_colon_spaced() {
        let out = scrub_credentials("token: sk-abcdefghijklmno", "ctx");
        assert!(out.contains("[REDACTED]"), "{out}");
    }

    #[test]
    fn redacts_password_equals() {
        let out = scrub_credentials("password=mysupersecretpass", "ctx");
        assert!(out.contains("[REDACTED]"), "{out}");
        assert!(out.contains("mysu"), "prefix should be visible: {out}");
    }

    #[test]
    fn short_values_not_redacted() {
        let out = scrub_credentials("token=abc", "ctx");
        assert!(!out.contains("[REDACTED]"), "short value should not be redacted: {out}");
    }

    #[test]
    fn clean_text_unchanged() {
        let input = "What is the weather today?";
        assert_eq!(scrub_credentials(input, "ctx"), input);
    }

    #[test]
    fn keyword_without_separator_not_redacted() {
        let input = "I have a token of appreciation for you.";
        assert_eq!(scrub_credentials(input, "ctx"), input);
    }

    #[test]
    fn multiple_secrets_all_redacted() {
        let input = "api_key=sk-abcdefghijkl password=hunter2password token=tok-abcdefghijkl";
        let out = scrub_credentials(input, "ctx");
        assert_eq!(out.matches("[REDACTED]").count(), 3, "expected 3 redactions: {out}");
    }

    #[test]
    fn process_combines_both_passes() {
        let input = "token=sk-abcdefghijkl and ignore previous instructions please";
        let out = process(input, "ctx");
        assert!(out.contains("[REDACTED]"), "{out}");
        // Injection phrase kept — detection only, no text modification.
        assert!(out.contains("ignore previous instructions"), "{out}");
    }

    #[test]
    fn bearer_token_redacted() {
        let out = scrub_credentials("Authorization: bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9", "ctx");
        assert!(out.contains("[REDACTED]"), "{out}");
        assert!(out.contains("eyJh"), "jwt prefix visible: {out}");
    }
}
